//! User settings: result preview size, result directory and result retention.
//!
//! Settings live in `settings.json` inside the data directory and hold no secrets. Command line
//! flags and environment variables override the file; documented defaults apply when neither is
//! set.
use crate::storage::{atomic_write, regular, restrict};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Rows a result set prints before it is stored and only previewed.
pub const DEFAULT_PREVIEW_ROWS: u64 = 10;
/// Encoded size a preview may reach before the remaining rows stay in the stored result.
pub const PREVIEW_BYTES: usize = 16 * 1024;
/// Hours a stored result is kept before the next command removes it.
pub const DEFAULT_RETENTION_HOURS: u64 = 24;
const MAX_PREVIEW_ROWS: u64 = 10_000;
const MAX_RETENTION_HOURS: u64 = 8_760;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key {
    PreviewRows,
    ResultsDir,
    RetentionHours,
}
impl Key {
    pub const ALL: [Key; 3] = [Key::PreviewRows, Key::ResultsDir, Key::RetentionHours];
    pub fn parse(name: &str) -> Result<Key> {
        Self::ALL
            .into_iter()
            .find(|key| key.name() == name)
            .with_context(|| {
                format!(
                    "unknown setting {name}; expected one of {}",
                    Self::ALL.map(Key::name).to_vec().join(", ")
                )
            })
    }
    pub fn name(self) -> &'static str {
        match self {
            Key::PreviewRows => "preview-rows",
            Key::ResultsDir => "results-dir",
            Key::RetentionHours => "results-retention-hours",
        }
    }
}
/// Where an effective value came from.
#[derive(Clone, Copy)]
pub enum Source {
    Default,
    File,
    Environment,
    Flag,
}
impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::File => "file",
            Source::Environment => "env",
            Source::Flag => "flag",
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(
        default,
        rename = "preview-rows",
        skip_serializing_if = "Option::is_none"
    )]
    pub preview_rows: Option<u64>,
    #[serde(
        default,
        rename = "results-dir",
        skip_serializing_if = "Option::is_none"
    )]
    pub results_dir: Option<String>,
    #[serde(
        default,
        rename = "results-retention-hours",
        skip_serializing_if = "Option::is_none"
    )]
    pub retention_hours: Option<u64>,
}
pub fn path(root: &Path) -> PathBuf {
    root.join("settings.json")
}
impl Settings {
    /// Read the settings file; a missing file means every default applies.
    pub fn load(root: &Path) -> Result<Self> {
        let path = path(root);
        if !path.exists() {
            return Ok(Self::default());
        }
        regular(&path)?;
        serde_json::from_slice(&fs::read(&path)?).context("invalid settings.json")
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        let path = path(root);
        if path.exists() {
            regular(&path)?;
        }
        atomic_write(&path, &serde_json::to_vec_pretty(self)?)
    }
    pub fn value(&self, key: Key) -> Option<String> {
        match key {
            Key::PreviewRows => self.preview_rows.map(|value| value.to_string()),
            Key::ResultsDir => self.results_dir.clone(),
            Key::RetentionHours => self.retention_hours.map(|value| value.to_string()),
        }
    }
    pub fn set(&mut self, key: Key, value: &str) -> Result<()> {
        match key {
            Key::PreviewRows => self.preview_rows = Some(number(value, MAX_PREVIEW_ROWS, key)?),
            Key::RetentionHours => {
                self.retention_hours = Some(number(value, MAX_RETENTION_HOURS, key)?)
            }
            Key::ResultsDir => self.results_dir = Some(directory(value)?),
        }
        Ok(())
    }
    pub fn unset(&mut self, key: Key) {
        match key {
            Key::PreviewRows => self.preview_rows = None,
            Key::ResultsDir => self.results_dir = None,
            Key::RetentionHours => self.retention_hours = None,
        }
    }
}
fn number(value: &str, max: u64, key: Key) -> Result<u64> {
    let parsed: u64 = value
        .trim()
        .parse()
        .with_context(|| format!("{} expects a whole number, not {value}", key.name()))?;
    if parsed > max {
        bail!("{} must be at most {max}", key.name());
    }
    Ok(parsed)
}
/// Accept an absolute path, expanding a leading `~`.
pub fn directory(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("results-dir must not be empty");
    }
    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        dirs::home_dir()
            .context("cannot resolve the home directory")?
            .join(rest)
    } else if trimmed == "~" {
        dirs::home_dir().context("cannot resolve the home directory")?
    } else {
        PathBuf::from(trimmed)
    };
    if !expanded.is_absolute() {
        bail!("results-dir must be an absolute path, not {trimmed}");
    }
    Ok(expanded.to_string_lossy().into_owned())
}
/// Default result directory: a per-user directory inside the system temporary directory.
///
/// It is intentionally outside the data directory, because temporary results should disappear
/// when the machine reboots; `sqlx setting set results-dir <path>` keeps them instead. The name
/// carries the owner and a digest of the data directory so two users on one machine never share a
/// directory, and the directory itself is rejected when it is a symbolic link or belongs to
/// somebody else.
pub fn default_results_dir(root: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let digest = hex::encode(Sha256::digest(canonical.to_string_lossy().as_bytes()));
    let owner = fs::metadata(root).ok().map(|meta| meta_uid(&meta));
    let name = match owner {
        Some(uid) => format!("sqlx-{uid}-{}", &digest[..8]),
        None => format!("sqlx-{}", &digest[..8]),
    };
    Ok(std::env::temp_dir().join(name).join("results"))
}
#[cfg(unix)]
fn meta_uid(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    meta.uid()
}
#[cfg(not(unix))]
fn meta_uid(_meta: &fs::Metadata) -> u32 {
    0
}
/// Create a result directory and make sure only the current user can read it.
pub fn ensure_results_dir(path: &Path, owned: bool, owner_source: &Path) -> Result<()> {
    if owned {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                bail!(
                    "result directory is a symbolic link; set another one with `sqlx setting set results-dir <path>`: {}",
                    path.display()
                );
            }
        }
    }
    fs::create_dir_all(path)
        .with_context(|| format!("cannot create the result directory {}", path.display()))?;
    if owned {
        let meta = fs::metadata(path)?;
        if meta_uid(&meta) != meta_uid(&fs::metadata(owner_source)?) {
            bail!(
                "result directory belongs to another user; set another one with `sqlx setting set results-dir <path>`: {}",
                path.display()
            );
        }
    }
    restrict(path, true)?;
    Ok(())
}
/// The settings in effect for one command.
pub struct Effective {
    pub preview_rows: u64,
    pub preview_source: Source,
    pub results_dir: PathBuf,
    pub results_dir_source: Source,
    /// `None` keeps stored results until the size limit removes them.
    pub retention: Option<u64>,
    pub retention_source: Source,
}
pub fn resolve(root: &Path, settings: &Settings, preview_flag: Option<u64>) -> Result<Effective> {
    let (preview_rows, preview_source) = match (
        preview_flag,
        environment_number("SQLX_PREVIEW_ROWS")?,
        settings.preview_rows,
    ) {
        (Some(value), _, _) => (value, Source::Flag),
        (None, Some(value), _) => (value, Source::Environment),
        (None, None, Some(value)) => (value, Source::File),
        (None, None, None) => (DEFAULT_PREVIEW_ROWS, Source::Default),
    };
    if preview_rows > MAX_PREVIEW_ROWS {
        bail!("preview-rows must be at most {MAX_PREVIEW_ROWS}");
    }
    let (results_dir, results_dir_source) =
        match (environment("SQLX_RESULTS_DIR")?, &settings.results_dir) {
            (Some(value), _) => (PathBuf::from(directory(&value)?), Source::Environment),
            (None, Some(value)) => (PathBuf::from(directory(value)?), Source::File),
            (None, None) => (default_results_dir(root)?, Source::Default),
        };
    let (retention_hours, retention_source) = match (
        environment_number("SQLX_RESULTS_RETENTION_HOURS")?,
        settings.retention_hours,
    ) {
        (Some(value), _) => (value, Source::Environment),
        (None, Some(value)) => (value, Source::File),
        (None, None) => (DEFAULT_RETENTION_HOURS, Source::Default),
    };
    if retention_hours > MAX_RETENTION_HOURS {
        bail!("results-retention-hours must be at most {MAX_RETENTION_HOURS}");
    }
    Ok(Effective {
        preview_rows,
        preview_source,
        results_dir,
        results_dir_source,
        retention: (retention_hours > 0).then(|| retention_hours * 60 * 60),
        retention_source,
    })
}
fn environment(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Ok(None),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn environment_number(name: &str) -> Result<Option<u64>> {
    match environment(name)? {
        Some(value) => {
            Ok(Some(value.trim().parse().with_context(|| {
                format!("{name} expects a whole number, not {value}")
            })?))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        dir
    }
    #[test]
    fn settings_round_trip_and_reject_unknown_keys() {
        let root = root();
        let results = std::env::temp_dir().join("sqlx-test-results");
        let mut settings = Settings::load(root.path()).unwrap();
        settings.set(Key::PreviewRows, "5").unwrap();
        settings
            .set(Key::ResultsDir, &results.to_string_lossy())
            .unwrap();
        settings.set(Key::RetentionHours, "0").unwrap();
        settings.save(root.path()).unwrap();
        let text = fs::read_to_string(path(root.path())).unwrap();
        assert!(text.contains("\"preview-rows\": 5"), "{text}");
        assert!(text.contains("\"results-retention-hours\": 0"), "{text}");
        let loaded = Settings::load(root.path()).unwrap();
        assert_eq!(loaded.preview_rows, Some(5));
        assert_eq!(
            loaded.results_dir.as_deref(),
            Some(results.to_string_lossy().as_ref())
        );
        fs::write(path(root.path()), "{\"preview-row\": 5}").unwrap();
        assert!(Settings::load(root.path()).is_err());
    }
    #[test]
    fn values_are_validated() {
        let mut settings = Settings::default();
        assert!(settings.set(Key::PreviewRows, "-1").is_err());
        assert!(settings.set(Key::PreviewRows, "abc").is_err());
        assert!(settings.set(Key::PreviewRows, "100000").is_err());
        assert!(settings.set(Key::ResultsDir, "relative/path").is_err());
        assert!(settings.set(Key::ResultsDir, "").is_err());
        settings.set(Key::ResultsDir, "~/sqlx-results").unwrap();
        let expanded = settings.results_dir.clone().unwrap();
        assert!(Path::new(&expanded).is_absolute(), "{expanded}");
    }
    #[test]
    fn resolution_prefers_flag_then_environment_then_file() {
        let root = root();
        let mut settings = Settings::default();
        settings.set(Key::PreviewRows, "3").unwrap();
        settings.set(Key::RetentionHours, "1").unwrap();
        let file = resolve(root.path(), &settings, None).unwrap();
        assert_eq!(file.preview_rows, 3);
        assert_eq!(file.retention, Some(3_600));
        assert_eq!(file.results_dir_source.name(), "default");
        let flag = resolve(root.path(), &settings, Some(7)).unwrap();
        assert_eq!(flag.preview_rows, 7);
        assert_eq!(flag.preview_source.name(), "flag");
        let environment_dir = std::env::temp_dir().join("sqlx-env-results");
        std::env::set_var("SQLX_PREVIEW_ROWS", "9");
        std::env::set_var("SQLX_RESULTS_DIR", &environment_dir);
        let environment = resolve(root.path(), &settings, None).unwrap();
        std::env::remove_var("SQLX_PREVIEW_ROWS");
        std::env::remove_var("SQLX_RESULTS_DIR");
        assert_eq!(environment.preview_rows, 9);
        assert_eq!(environment.preview_source.name(), "env");
        assert_eq!(environment.results_dir, environment_dir);
        assert_eq!(environment.results_dir_source.name(), "env");
    }
    #[test]
    fn the_default_result_directory_is_private_and_temporary() {
        let root = root();
        let first = default_results_dir(root.path()).unwrap();
        let second = default_results_dir(root.path()).unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with(std::env::temp_dir()));
        ensure_results_dir(&first, true, root.path()).unwrap();
        let meta = fs::symlink_metadata(&first).unwrap();
        assert!(!meta.file_type().is_symlink());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        }
        fs::remove_dir_all(first.parent().unwrap()).unwrap();
    }
}
