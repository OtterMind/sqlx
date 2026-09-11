//! Checks and executable replacement. This module never opens database storage.
use crate::{
    components::{self, Asset, Manifest},
    storage::{atomic_write, open_private, regular, restrict},
};
use anyhow::{Context, Result};
use fs2::FileExt;
use reqwest::{blocking::Client, Url};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const RELEASE_BASE: &str = "https://github.com/OtterMind/sqlx/releases";
const CHECK_INTERVAL: u64 = 24 * 60 * 60;
const FAILURE_BACKOFF: u64 = 15 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateError {
    pub code: String,
    pub message: String,
}
impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for UpdateError {}
fn failure(code: &str, message: impl std::fmt::Display) -> anyhow::Error {
    UpdateError {
        code: format!("update.{code}"),
        message: message.to_string(),
    }
    .into()
}
fn problem(error: &anyhow::Error) -> UpdateError {
    error
        .downcast_ref::<UpdateError>()
        .cloned()
        .unwrap_or_else(|| UpdateError {
            code: "update.failed".into(),
            message: format!("{error:#}"),
        })
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Serialize, Deserialize)]
struct CheckRecord {
    source: String,
    current_version: String,
    checked_at: u64,
    status: String,
    latest_version: Option<String>,
    release_url: Option<String>,
    error: Option<UpdateError>,
    #[serde(default)]
    notified_at: Option<u64>,
    #[serde(default)]
    notified_version: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct InstallRecord {
    status: String,
    from_version: String,
    to_version: Option<String>,
    installed_path: PathBuf,
    started_at: u64,
    finished_at: Option<u64>,
    backup_path: Option<PathBuf>,
    previous_sha256: Option<String>,
    error: Option<UpdateError>,
}
struct Release {
    version: Version,
    asset: Asset,
    page: String,
}

pub struct Updater {
    target: PathBuf,
    state_root: PathBuf,
    dir: PathBuf,
    base: String,
    current: Version,
}
impl Updater {
    pub fn new(state_root: Option<PathBuf>, base: String) -> Result<Self> {
        let url = valid_url(base.trim_end_matches('/'))?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err(failure(
                "invalid_source",
                "release base must not contain a query or fragment",
            ));
        }
        let base = url.as_str().trim_end_matches('/').to_owned();
        let target = std::env::current_exe()?.canonicalize()?;
        let state_root = state_root.unwrap_or(
            dirs::home_dir()
                .context("cannot locate user home directory")?
                .join(".sqlx/updates"),
        );
        let dir = state_root.join(hex::encode(Sha256::digest(
            target.to_string_lossy().as_bytes(),
        )));
        Ok(Self {
            target,
            state_root,
            dir,
            base,
            current: Version::parse(env!("CARGO_PKG_VERSION"))?,
        })
    }
    fn prepare_state(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        if fs::symlink_metadata(&self.dir)?.file_type().is_symlink() {
            return Err(failure(
                "invalid_state",
                "update state must not be a symbolic link",
            ));
        }
        restrict(&self.dir, true)
    }
    fn client(background: bool) -> Result<Client> {
        Ok(Client::builder()
            .user_agent(concat!("OtterMind-SQLX/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(if background { 2 } else { 5 }))
            .timeout(Duration::from_secs(if background { 5 } else { 15 }))
            .build()?)
    }
    fn release(&self, version: Option<&str>, background: bool) -> Result<Release> {
        let http = Self::client(background)?;
        let version = match version {
            Some(value) => stable_version(value)?,
            None => stable_version(&fetch_text(
                &http,
                &format!("{}/latest/download/release-version.txt", self.base),
            )?)?,
        };
        let prefix = format!("{}/download/v{version}/", self.base);
        let manifest: Manifest =
            serde_json::from_str(&fetch_text(&http, &format!("{prefix}manifest.json"))?)
                .map_err(|e| failure("invalid_manifest", e))?;
        if manifest.schema_version != 1 {
            return Err(failure(
                "unsupported_manifest",
                "use the official installer for this release format",
            ));
        }
        let platform = components::platform()?;
        let asset = manifest
            .components
            .get(&format!("cli:{platform}"))
            .cloned()
            .ok_or_else(|| {
                failure(
                    "unsupported_platform",
                    "this release does not contain a CLI for this platform",
                )
            })?;
        // cli_compat and protocol_version describe components used by the new CLI,
        // not whether this updater can install that executable.
        let expected_entry = format!("sqlx{}", std::env::consts::EXE_SUFFIX);
        let expected_url = format!("{prefix}sqlx-{platform}.zip");
        if asset.version != version.to_string()
            || asset.entrypoint != expected_entry
            || asset.archive != "zip"
            || asset.url != expected_url
        {
            return Err(failure(
                "invalid_manifest",
                "CLI version, archive or URL does not match the selected release",
            ));
        }
        if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(failure("invalid_manifest", "invalid CLI checksum"));
        }
        Ok(Release {
            page: format!("{}/tag/v{version}", self.base),
            version,
            asset,
        })
    }
    pub fn check(&self, background: bool) -> Result<Value> {
        self.prepare_state()?;
        let lock = open_private(&self.dir.join("check.lock"))?;
        if background {
            if lock.try_lock_exclusive().is_err() || !self.check_due() {
                return Ok(json!({"status":"skipped"}));
            }
        } else {
            lock.lock_exclusive()?;
        }
        let previous = read_record::<CheckRecord>(&self.dir.join("check.json"))
            .ok()
            .flatten()
            .filter(|record| record.source == self.base);
        let mut record = CheckRecord {
            source: self.base.clone(),
            current_version: self.current.to_string(),
            checked_at: now(),
            status: "checking".into(),
            latest_version: None,
            release_url: None,
            error: None,
            notified_at: previous.as_ref().and_then(|record| record.notified_at),
            notified_version: previous.and_then(|record| record.notified_version),
        };
        atomic_write(&self.dir.join("check.json"), &serde_json::to_vec(&record)?)?;
        let outcome = self.release(None, background);
        match &outcome {
            Ok(release) => {
                record.status = if release.version.cmp_precedence(&self.current).is_gt() {
                    "update_available"
                } else {
                    "up_to_date"
                }
                .into();
                record.latest_version = Some(release.version.to_string());
                record.release_url = Some(release.page.clone());
            }
            Err(error) => {
                record.status = "check_failed".into();
                record.error = Some(problem(error));
            }
        }
        record.checked_at = now();
        atomic_write(&self.dir.join("check.json"), &serde_json::to_vec(&record)?)?;
        outcome?;
        Ok(serde_json::to_value(record)?)
    }
    fn check_due(&self) -> bool {
        let record = read_record::<CheckRecord>(&self.dir.join("check.json"))
            .ok()
            .flatten();
        !record.is_some_and(|r| {
            r.source == self.base
                && r.current_version == self.current.to_string()
                && now().saturating_sub(r.checked_at)
                    < if matches!(r.status.as_str(), "update_available" | "up_to_date") {
                        CHECK_INTERVAL
                    } else {
                        FAILURE_BACKOFF
                    }
        })
    }
    /// Called only for interactive invocations. Errors must not change the command outcome.
    pub fn notify_and_schedule(&self) {
        let _ = self.notify_cached();
        if !self.check_due() {
            return;
        }
        let mut child = Command::new(&self.target);
        child
            .args(["--update-release-base", &self.base])
            .arg("--update-dir")
            .arg(&self.state_root)
            .args(["update", "background-check"])
            .env("SQLX_NO_UPDATE_CHECK", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = crate::process::spawn_detached(&mut child);
    }
    fn notify_cached(&self) -> Result<()> {
        if !self.dir.exists() {
            return Ok(());
        }
        let lock = open_private(&self.dir.join("check.lock"))?;
        if lock.try_lock_exclusive().is_err() {
            return Ok(());
        }
        if let Some(mut record) = read_record::<CheckRecord>(&self.dir.join("check.json"))? {
            if record.source == self.base
                && record.current_version == self.current.to_string()
                && record.status == "update_available"
                && (record.notified_version != record.latest_version
                    || record
                        .notified_at
                        .is_none_or(|at| now().saturating_sub(at) >= CHECK_INTERVAL))
            {
                eprintln!(
                    "SQLX {} is available. Run: sqlx update install",
                    record.latest_version.as_deref().unwrap_or("update")
                );
                record.notified_at = Some(now());
                record.notified_version = record.latest_version.clone();
                atomic_write(&self.dir.join("check.json"), &serde_json::to_vec(&record)?)?;
            }
        }
        Ok(())
    }
    pub fn status(&self) -> Result<Value> {
        Ok(
            json!({"current_version":self.current.to_string(),"installed_path":self.target,
            "last_check":read_record::<CheckRecord>(&self.dir.join("check.json"))?,
            "last_install":read_record::<InstallRecord>(&self.dir.join("install.json"))?}),
        )
    }
    pub fn install(&self, requested: Option<&str>) -> Result<Value> {
        self.prepare_state()?;
        let lock = open_private(&self.dir.join("install.lock"))?;
        lock.try_lock_exclusive().map_err(|_| {
            failure(
                "in_progress",
                "another update is already in progress for this executable",
            )
        })?;
        let mut record = InstallRecord {
            status: "preparing".into(),
            from_version: self.current.to_string(),
            to_version: requested.map(str::to_owned),
            installed_path: self.target.clone(),
            started_at: now(),
            finished_at: None,
            backup_path: None,
            previous_sha256: None,
            error: None,
        };
        let outcome = self.install_inner(requested, &mut record);
        if let Err(error) = &outcome {
            if record.status != "recovery_required" {
                record.status = "failed".into();
            }
            record.error = Some(problem(error));
        }
        record.finished_at = Some(now());
        if record.status == "installed" {
            record.backup_path = None;
        }
        if let Err(error) = atomic_write(
            &self.dir.join("install.json"),
            &serde_json::to_vec(&record)?,
        ) {
            return Err(outcome.err().unwrap_or_else(|| failure("receipt_failed", format!("CLI outcome: {}. Could not save update history; recovery files were retained: {error}", record.status))));
        }
        if let Some(stage) = outcome? {
            let _ = fs::remove_dir_all(stage);
        }
        Ok(
            json!({"status":record.status,"from_version":record.from_version,"to_version":record.to_version,
            "installed_path":record.installed_path,"next":"Use sqlx skill update to update managed Skills."}),
        )
    }
    fn install_inner(
        &self,
        requested: Option<&str>,
        record: &mut InstallRecord,
    ) -> Result<Option<PathBuf>> {
        let release = self.release(requested, false)?;
        record.to_version = Some(release.version.to_string());
        let comparison = release.version.cmp_precedence(&self.current);
        if requested.is_some() && comparison.is_lt() {
            return Err(failure("downgrade_not_supported", "automatic downgrades are not supported; use the official installer to choose an older release"));
        }
        if !comparison.is_gt() {
            record.to_version = Some(self.current.to_string());
            record.status = "up_to_date".into();
            return Ok(None);
        }
        self.check_installation()?;
        let before = components::hash(&self.target)?;
        let parent = self
            .target
            .parent()
            .context("executable has no parent directory")?;
        let stage = tempfile::Builder::new()
            .prefix(".sqlx-update-")
            .tempdir_in(parent)
            .map_err(|e| failure("unwritable_installation", e))?;
        let stage = stage.keep();
        let backup = stage.join(format!("previous{}", std::env::consts::EXE_SUFFIX));
        let outcome = (|| -> Result<()> {
            let sums = fetch_text(
                &Self::client(false)?,
                &format!("{}/download/v{}/SHA256SUMS", self.base, release.version),
            )?;
            let name = release.asset.url.rsplit('/').next().unwrap();
            let entries: Vec<_> = sums
                .lines()
                .filter_map(|line| {
                    let mut p = line.split_whitespace();
                    Some((p.next()?, p.next()?))
                })
                .filter(|(_, file)| *file == name)
                .collect();
            if entries.len() != 1 || !entries[0].0.eq_ignore_ascii_case(&release.asset.sha256) {
                return Err(failure(
                    "checksum_mismatch",
                    "release manifest and SHA256SUMS disagree",
                ));
            }
            let archive = stage.join("download.zip");
            let http = Client::builder()
                .user_agent(concat!("OtterMind-SQLX/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(300))
                .build()?;
            let mut file = open_private(&archive)?;
            http.get(valid_url(&release.asset.url)?)
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|e| failure("download_failed", e))?
                .copy_to(&mut file)
                .map_err(|e| failure("download_failed", e))?;
            file.sync_all()?;
            drop(file);
            if !components::hash(&archive)?.eq_ignore_ascii_case(&release.asset.sha256) {
                return Err(failure(
                    "checksum_mismatch",
                    "CLI download checksum verification failed",
                ));
            }
            let unpacked = stage.join("unpacked");
            fs::create_dir(&unpacked)?;
            components::unzip(&archive, &unpacked).map_err(|e| failure("invalid_archive", e))?;
            let candidate = unpacked.join(&release.asset.entrypoint);
            regular(&candidate).map_err(|e| failure("invalid_archive", e))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700))?;
            }
            verify_version(&candidate, &release.version, &stage)?;
            if components::hash(&self.target)? != before {
                return Err(failure(
                    "target_changed",
                    "the installed executable changed while the update was downloading",
                ));
            }
            fs::copy(&self.target, &backup)?;
            if components::hash(&backup)? != before {
                return Err(failure(
                    "target_changed",
                    "the installed executable changed while it was backed up",
                ));
            }
            record.backup_path = Some(backup.clone());
            record.previous_sha256 = Some(before.clone());
            record.status = "installing".into();
            atomic_write(
                &self.dir.join("install.json"),
                &serde_json::to_vec(&record)?,
            )?;
            let installed = self_replace::self_replace(&candidate)
                .map_err(|e| failure("replace_failed", e))
                .and_then(|_| verify_version(&self.target, &release.version, &stage));
            if let Err(error) = installed {
                // Keep this independent backup: the library may have relocated and
                // scheduled deletion of the running image before returning an error.
                let restored = restore(&backup, &self.target, &before);
                if let Err(restore_error) = restored {
                    record.status = "recovery_required".into();
                    return Err(failure("recovery_required", format!("{error}; could not restore the previous executable: {restore_error}. Backup: {}", backup.display())));
                }
                record.backup_path = None;
                return Err(error.context("the previous executable was restored"));
            }
            record.status = "installed".into();
            Ok(())
        })();
        // The final receipt must be durable before deleting the recovery copy.
        if outcome.is_ok() {
            return Ok(Some(stage));
        }
        if record.status != "recovery_required" {
            let _ = fs::remove_dir_all(stage);
        }
        outcome.map(|_| None)
    }
    fn check_installation(&self) -> Result<()> {
        regular(&self.target)?;
        let path = self
            .target
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if path.contains("/target/debug/")
            || path.contains("/target/release/")
            || path.contains("/cellar/")
            || path.starts_with("/nix/store/")
            || path.starts_with("/snap/")
            || path.starts_with("/usr/bin/")
            || path.starts_with("/bin/")
            || path.contains("/windowsapps/")
            || path.contains("/scoop/apps/")
            || path.contains("/chocolatey/")
        {
            return Err(failure(
                "managed_installation",
                "use the source build or package manager that owns this executable",
            ));
        }
        if fs::metadata(&self.target)?.permissions().readonly() {
            return Err(failure(
                "unwritable_installation",
                "the installed executable is read-only; use its installer or package manager",
            ));
        }
        Ok(())
    }
}

fn valid_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|e| failure("invalid_source", e))?;
    if !(url.scheme() == "https"
        || (url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(failure(
            "invalid_source",
            "update downloads require HTTPS; loopback HTTP is allowed for local tests",
        ));
    }
    Ok(url)
}
fn stable_version(value: &str) -> Result<Version> {
    let version = Version::parse(value.trim().strip_prefix('v').unwrap_or(value.trim()))
        .map_err(|e| failure("invalid_version", e))?;
    if !version.pre.is_empty() {
        return Err(failure(
            "prerelease_not_supported",
            "only stable releases are eligible for updates",
        ));
    }
    Ok(version)
}
fn fetch_text(http: &Client, url: &str) -> Result<String> {
    let response = http
        .get(valid_url(url)?)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| failure("check_failed", e))?;
    let mut bytes = Vec::new();
    response
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| failure("check_failed", e))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(failure("invalid_manifest", "release metadata is too large"));
    }
    String::from_utf8(bytes).map_err(|e| failure("invalid_manifest", e))
}
fn read_record<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    regular(path)?;
    Ok(Some(
        serde_json::from_slice(&fs::read(path)?).map_err(|e| failure("invalid_state", e))?,
    ))
}
fn verify_version(binary: &Path, version: &Version, stage: &Path) -> Result<()> {
    let mut output = tempfile::NamedTempFile::new_in(stage)?;
    let mut child = Command::new(binary)
        .arg("--version")
        .env("SQLX_NO_UPDATE_CHECK", "1")
        .stdin(Stdio::null())
        .stdout(output.as_file().try_clone()?)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| failure("verification_failed", e))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failure(
                "verification_failed",
                "candidate version verification timed out",
            ));
        }
        thread::sleep(Duration::from_millis(25));
    };
    output.as_file_mut().seek(SeekFrom::Start(0))?;
    let mut text = String::new();
    output.as_file_mut().take(1025).read_to_string(&mut text)?;
    if !status.success() || text.trim() != format!("sqlx {version} (OtterMind/sqlx)") {
        return Err(failure(
            "verification_failed",
            "executable identity or version does not match the selected release",
        ));
    }
    Ok(())
}
fn restore(backup: &Path, target: &Path, expected: &str) -> Result<()> {
    if components::hash(target).is_ok_and(|digest| digest == expected) {
        return Ok(());
    }
    if components::hash(backup)? != expected {
        return Err(failure(
            "recovery_required",
            "backup checksum verification failed",
        ));
    }
    // The new process used for verification has exited; replace the failed candidate.
    #[cfg(windows)]
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(backup, target)?;
    if components::hash(target)? != expected {
        return Err(failure(
            "recovery_required",
            "restored executable checksum verification failed",
        ));
    }
    Ok(())
}
