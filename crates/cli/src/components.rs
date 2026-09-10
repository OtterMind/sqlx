use crate::storage::{atomic_write, open_private};
use anyhow::{anyhow, bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub const DEFAULT_MANIFEST: &str =
    "https://github.com/OtterMind/sqlx/releases/latest/download/manifest.json";
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
            "{}.json",
            hex::encode(Sha256::digest(self.source.as_bytes()))
        ));
        let bytes = if cache.exists() && !refresh {
            fs::read(&cache)?
        } else {
            let mut response = client()?
                .get(valid_url(&self.source)?)
                .send()?
                .error_for_status()
                .context(
                    "release manifest is unavailable; publish release assets or supply --manifest",
                )?;
            let mut bytes = Vec::new();
            response.read_to_end(&mut bytes)?;
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
        validate_asset(asset)?;
        let category = match name {
            "skill" => "skills",
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
            return Ok(target.join(&asset.entrypoint));
        }
        eprintln!("Downloading {name} {} for {platform}", asset.version);
        let mut download = tempfile::NamedTempFile::new_in(&parent)?;
        client()?
            .get(valid_url(&asset.url)?)
            .send()?
            .error_for_status()?
            .copy_to(&mut download)?;
        download.flush()?;
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
        Ok(target.join(&asset.entrypoint))
    }
}
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("OtterMind-SQLX/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()?)
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
fn unzip(source: &Path, target: &Path) -> Result<()> {
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
}
