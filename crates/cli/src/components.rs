use crate::storage::{atomic_write, open_private};
use anyhow::{anyhow, bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{IsTerminal, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

pub const DEFAULT_MANIFEST: &str = concat!(
    "https://github.com/OtterMind/sqlx/releases/download/v",
    env!("CARGO_PKG_VERSION"),
    "/manifest.json"
);
#[derive(Clone, Serialize, Deserialize)]
pub struct Asset {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub archive: String,
    pub entrypoint: String,
    #[serde(default)]
    pub cli_compat: String,
    #[serde(default = "protocol_version")]
    pub protocol_version: u32,
}
fn protocol_version() -> u32 {
    1
}
#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub components: BTreeMap<String, Asset>,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    asset: Asset,
    files: BTreeMap<String, String>,
}
pub struct Components {
    pub root: PathBuf,
    pub source: String,
}
impl Components {
    pub fn new(root: PathBuf, source: String) -> Self {
        Self { root, source }
    }
    pub fn manifest(&self, refresh: bool) -> Result<Manifest> {
        let dir = self.root.join("manifests");
        fs::create_dir_all(&dir)?;
        let cache = dir.join(format!(
            "{}-{}.json",
            hex::encode(Sha256::digest(self.source.as_bytes())),
            env!("CARGO_PKG_VERSION")
        ));
        let bytes = if cache.exists() && !refresh {
            fs::read(&cache)?
        } else {
            let url = valid_url(&self.source)?;
            let bytes = with_retry("Release manifest download", || {
                eprintln!("Downloading release manifest");
                let mut response = client()?.get(url.clone()).send()?.error_for_status()?;
                let mut bytes = Vec::new();
                response.read_to_end(&mut bytes)?;
                Ok(bytes)
            })
            .context(
                "release manifest is unavailable; publish release assets or supply --manifest",
            )?;
            let parsed: Manifest = serde_json::from_slice(&bytes)?;
            validate_manifest(&parsed)?;
            atomic_write(&cache, &bytes)?;
            bytes
        };
        let parsed: Manifest = serde_json::from_slice(&bytes)?;
        validate_manifest(&parsed)?;
        Ok(parsed)
    }
    pub fn asset<'a>(
        &self,
        manifest: &'a Manifest,
        name: &str,
        platform: &str,
    ) -> Result<&'a Asset> {
        let key = format!("{name}:{platform}");
        let asset = manifest
            .components
            .get(&key)
            .ok_or_else(|| anyhow!("release does not contain component {key}"))?;
        validate_asset(asset)?;
        Ok(asset)
    }
    pub fn ensure(&self, name: &str, platform: &str, asset: &Asset) -> Result<PathBuf> {
        Ok(self.ensure_with_status(name, platform, asset)?.0)
    }
    /// Install a component when it is missing; the flag reports whether it was downloaded now.
    pub fn ensure_with_status(
        &self,
        name: &str,
        platform: &str,
        asset: &Asset,
    ) -> Result<(PathBuf, bool)> {
        validate_asset(asset)?;
        let category = match name {
            "skill" => "skills",
            "ui-default" => "plugin-packages",
            "java" => "runtimes",
            "jdbc" | "ui" => "engines",
            _ => "drivers",
        };
        let component_name = if name == "skill" { "sqlx" } else { name };
        let parent = self.root.join(category).join(component_name).join(platform);
        fs::create_dir_all(&parent)?;
        let lock = open_private(&parent.join("install.lock"))?;
        lock.lock_exclusive()?;
        let target = parent.join(&asset.version);
        if target.exists() {
            let receipt: Receipt = serde_json::from_slice(&fs::read(target.join(".receipt.json"))?)
                .context("component installation is incomplete")?;
            if receipt.asset.sha256 != asset.sha256 || receipt.asset.entrypoint != asset.entrypoint
            {
                bail!("same component version refers to different content; publish a new version");
            }
            if files(&target)? != receipt.files {
                bail!("installed component integrity check failed; remove only the damaged component directory and retry");
            }
            return Ok((target.join(&asset.entrypoint), false));
        }
        let label = format!("{name} {} for {platform}", asset.version);
        let mut download = tempfile::NamedTempFile::new_in(&parent)?;
        download_to(&asset.url, download.as_file_mut(), &label)
            .with_context(|| format!("cannot install {label}"))?;
        if hash(download.path())? != asset.sha256.to_ascii_lowercase() {
            bail!("download checksum mismatch for {name}");
        }
        let stage = tempfile::tempdir_in(&parent)?;
        match asset.archive.as_str() {
            "zip" => unzip(download.path(), stage.path())?,
            "tar.gz" => untar(download.path(), stage.path())?,
            "file" => {
                let entry = safe_join(stage.path(), &asset.entrypoint)?;
                fs::create_dir_all(entry.parent().unwrap())?;
                fs::copy(download.path(), entry)?;
            }
            _ => bail!("unsupported component archive format"),
        }
        let entry = safe_join(stage.path(), &asset.entrypoint)?;
        if !entry.is_file() {
            bail!("component entrypoint is missing");
        }
        let canonical = entry.canonicalize()?;
        if !canonical.starts_with(stage.path().canonicalize()?) {
            bail!("component entrypoint escapes archive");
        }
        #[cfg(unix)]
        if name == "mysql" || name == "postgres" || name == "java" || name == "ui" {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&entry, fs::Permissions::from_mode(0o700))?;
        }
        let receipt = Receipt {
            asset: asset.clone(),
            files: files(stage.path())?,
        };
        atomic_write(
            &stage.path().join(".receipt.json"),
            &serde_json::to_vec(&receipt)?,
        )?;
        fs::rename(stage.path(), &target)?;
        Ok((target.join(&asset.entrypoint), true))
    }
}
pub fn download_plugin(url: &str, sha256: &str, parent: &Path) -> Result<tempfile::TempDir> {
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("a valid SHA-256 is required for a plugin download");
    }
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    download_to(url, file.as_file_mut(), "UI plugin").context("cannot install the UI plugin")?;
    if hash(file.path())? != sha256.to_ascii_lowercase() {
        bail!("UI plugin checksum mismatch");
    }
    let unpacked = tempfile::tempdir_in(parent)?;
    unzip(file.path(), unpacked.path())?;
    Ok(unpacked)
}
const DOWNLOAD_ATTEMPTS: u32 = 3;
const RETRY_BACKOFF: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];
const READ_CHUNK: usize = 65536;
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("OtterMind-SQLX/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        // The blocking client has no per-read timeout, so the total budget must fit a slow
        // but progressing transfer (a 5 MB worker at 3 KB/s is ~28 minutes); failures are retried.
        .timeout(Duration::from_secs(1800))
        .build()?)
}
fn with_retry<T>(what: &str, mut action: impl FnMut() -> Result<T>) -> Result<T> {
    let mut attempt = 1;
    loop {
        match action() {
            Ok(value) => return Ok(value),
            Err(error) if attempt < DOWNLOAD_ATTEMPTS && retryable(&error) => {
                let backoff = RETRY_BACKOFF[(attempt - 1) as usize];
                eprintln!(
                    "{what} failed ({error}); retrying in {}s ({attempt}/{DOWNLOAD_ATTEMPTS})",
                    backoff.as_secs()
                );
                std::thread::sleep(backoff);
                attempt += 1;
            }
            Err(error) => {
                let attempts = if attempt > 1 {
                    format!(" after {attempt} attempts")
                } else {
                    String::new()
                };
                return Err(error.context(format!(
                    "{what} failed{attempts}; re-run the same command to retry"
                )));
            }
        }
    }
}
/// A stalled or interrupted transfer is worth retrying; an HTTP status error is not.
fn retryable(error: &anyhow::Error) -> bool {
    if let Some(http) = error.downcast_ref::<reqwest::Error>() {
        return http.status().is_none();
    }
    error.downcast_ref::<std::io::Error>().is_some()
}
fn download_to(url: &str, dest: &mut File, label: &str) -> Result<()> {
    let url = valid_url(url)?;
    with_retry("Download", || transfer(&url, dest, label))
        .with_context(|| format!("cannot download {label}"))
}
fn transfer(url: &reqwest::Url, dest: &mut File, label: &str) -> Result<()> {
    dest.set_len(0)?;
    dest.seek(SeekFrom::Start(0))?;
    eprintln!("Downloading {label}");
    let mut response = client()?.get(url.clone()).send()?.error_for_status()?;
    let total = response.content_length();
    let interactive = std::io::stderr().is_terminal();
    let started = Instant::now();
    let mut done = 0u64;
    let mut buffer = vec![0u8; READ_CHUNK];
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        dest.write_all(&buffer[..read])?;
        done += read as u64;
        if interactive {
            eprint!("\r{}", progress_line(label, done, total, started.elapsed()));
        }
    }
    dest.flush()?;
    let elapsed = started.elapsed();
    if interactive {
        eprint!("\r\x1b[K");
    }
    eprintln!(
        "Downloaded {label} in {} ({})",
        human_duration(elapsed),
        human_rate(done, elapsed)
    );
    Ok(())
}
fn progress_line(label: &str, done: u64, total: Option<u64>, elapsed: Duration) -> String {
    let rate = human_rate(done, elapsed);
    match total {
        Some(total) if total > 0 => {
            let percent = done.saturating_mul(100) / total;
            let remaining = total.saturating_sub(done) as f64;
            let eta =
                Duration::from_secs_f64(elapsed.as_secs_f64() * remaining / done.max(1) as f64);
            format!(
                "  {label}: {} / {} ({percent}%) {rate} ETA {}",
                human_size(done),
                human_size(total),
                human_duration(eta)
            )
        }
        _ => format!("  {label}: {} {rate}", human_size(done)),
    }
}
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
pub(crate) fn human_duration(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    if seconds < 60.0 {
        return format!("{seconds:.1}s");
    }
    let mut minutes = (seconds / 60.0).floor() as u64;
    let mut rest = seconds - minutes as f64 * 60.0;
    if rest.round() >= 60.0 {
        minutes += 1;
        rest = 0.0;
    }
    format!("{minutes}m {:02}s", rest.round() as u64)
}
pub(crate) fn human_rate(bytes: u64, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return format!("{}/s", human_size(bytes));
    }
    format!("{}/s", human_size((bytes as f64 / seconds) as u64))
}
fn valid_url(s: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(s)?;
    if url.scheme() != "https"
        && !(url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
    {
        bail!("downloads require HTTPS (HTTP is allowed only for local tests)");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("download URL must not include credentials");
    }
    Ok(url)
}
fn validate_manifest(m: &Manifest) -> Result<()> {
    if m.schema_version != 1 {
        bail!("unsupported release manifest version");
    }
    Ok(())
}
fn validate_asset(a: &Asset) -> Result<()> {
    semver::Version::parse(&a.version).context("invalid component version")?;
    if a.protocol_version != 1 {
        bail!("incompatible worker protocol version");
    }
    if !a.cli_compat.is_empty()
        && !semver::VersionReq::parse(&a.cli_compat)?
            .matches(&semver::Version::parse(env!("CARGO_PKG_VERSION"))?)
    {
        bail!("component requires CLI {}", a.cli_compat);
    }
    if a.sha256.len() != 64 || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("invalid component SHA-256");
    }
    safe_join(Path::new("."), &a.entrypoint)?;
    Ok(())
}
pub fn platform() -> Result<String> {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        _ => bail!("unsupported operating system"),
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => bail!("unsupported CPU architecture"),
    };
    if os == "windows" && arch != "x64" {
        bail!("Windows ARM64 is not a v1 release target");
    }
    Ok(format!("{os}-{arch}"))
}
pub fn safe_join(root: &Path, name: &str) -> Result<PathBuf> {
    let p = Path::new(name);
    if name.is_empty()
        || name.contains('\\')
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("unsafe archive path");
    }
    Ok(root.join(p))
}
pub fn hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
pub fn files(root: &Path) -> Result<BTreeMap<String, String>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        for item in fs::read_dir(dir)? {
            let p = item?.path();
            let name = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            if name == ".receipt.json" {
                continue;
            }
            let ty = fs::symlink_metadata(&p)?.file_type();
            if ty.is_symlink() {
                out.insert(name, format!("symlink:{}", fs::read_link(&p)?.display()));
            } else if ty.is_dir() {
                visit(root, &p, out)?;
            } else if ty.is_file() {
                out.insert(name, hash(&p)?);
            } else {
                bail!("unsupported file in component");
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out)?;
    Ok(out)
}
pub(crate) fn unzip(source: &Path, target: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(source)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().trim_end_matches('/');
        if name.is_empty() {
            continue;
        }
        let path = safe_join(target, name)?;
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            bail!("ZIP symbolic links are not supported");
        }
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
        } else {
            fs::create_dir_all(path.parent().unwrap())?;
            let mut f = File::options().write(true).create_new(true).open(&path)?;
            std::io::copy(&mut entry, &mut f)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))?;
            }
        }
    }
    Ok(())
}
fn untar(source: &Path, target: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(File::open(source)?);
    let mut archive = tar::Archive::new(decoder);
    let mut links = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry
            .path()?
            .to_string_lossy()
            .trim_start_matches("./")
            .trim_end_matches('/')
            .to_string();
        if name.is_empty() {
            continue;
        }
        let path = safe_join(target, &name)?;
        let ty = entry.header().entry_type();
        if ty.is_dir() {
            fs::create_dir_all(path)?;
        } else if ty.is_file() {
            fs::create_dir_all(path.parent().unwrap())?;
            let mut f = File::options().write(true).create_new(true).open(&path)?;
            std::io::copy(&mut entry, &mut f)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(entry.header().mode()? & 0o777),
                )?;
            }
        } else if ty.is_symlink() || ty.is_hard_link() {
            let link = entry
                .link_name()?
                .context("missing archive link target")?
                .into_owned();
            links.push((path, link, ty.is_hard_link()));
        } else if !ty.is_pax_global_extensions() && !ty.is_pax_local_extensions() {
            bail!("unsupported tar entry");
        }
    }
    // Install links last, so archive files cannot be written through a symlink.
    for (path, link, hard) in links {
        if link.is_absolute() {
            bail!("absolute archive link");
        }
        let parent = if hard { target } else { path.parent().unwrap() };
        let resolved = parent
            .join(&link)
            .canonicalize()
            .context("archive link target missing")?;
        if !resolved.starts_with(target.canonicalize()?) {
            bail!("archive link escapes component");
        }
        fs::create_dir_all(path.parent().unwrap())?;
        if hard {
            fs::hard_link(resolved, path)?;
        } else {
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, path)?;
            #[cfg(not(unix))]
            {
                fs::copy(resolved, path)?;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_cannot_escape_component() {
        for path in ["../key", "/tmp/key", "a/../../key", "a\\..\\key", ""] {
            assert!(safe_join(Path::new("cache"), path).is_err());
        }
    }
    #[test]
    fn sizes_and_durations_stay_readable() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(4_731_712), "4.7 MB");
        assert_eq!(human_duration(Duration::from_millis(1_500)), "1.5s");
        assert_eq!(human_duration(Duration::from_secs(59)), "59.0s");
        assert_eq!(human_duration(Duration::from_secs(60)), "1m 00s");
        assert_eq!(human_duration(Duration::from_millis(119_600)), "2m 00s");
        assert_eq!(human_duration(Duration::from_secs(185)), "3m 05s");
        assert_eq!(human_rate(1_000_000, Duration::from_secs(2)), "500.0 KB/s");
    }
    #[test]
    fn progress_reports_percent_and_eta_only_with_a_total() {
        let with_total = progress_line(
            "mysql 0.1.5 for macos-arm64",
            2_500_000,
            Some(5_000_000),
            Duration::from_secs(2),
        );
        assert!(with_total.contains("2.5 MB / 5.0 MB"), "{with_total}");
        assert!(with_total.contains("(50%)"), "{with_total}");
        assert!(with_total.contains("ETA 2.0s"), "{with_total}");
        let without_total = progress_line("mysql 0.1.5", 1_500, None, Duration::from_secs(1));
        assert_eq!(without_total, "  mysql 0.1.5: 1.5 KB 1.5 KB/s");
    }
    #[test]
    fn only_transport_failures_are_retried() {
        let transport = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "connection stalled",
        ));
        assert!(retryable(&transport));
        let application = anyhow::anyhow!("download checksum mismatch for mysql");
        assert!(!retryable(&application));
    }
}
