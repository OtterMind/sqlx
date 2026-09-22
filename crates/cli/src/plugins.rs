//! Static UI plugins. Plugins own presentation; the local service owns credentials and execution.
use crate::{
    components::{self, files, hash, safe_join, Components},
    storage::{atomic_write, open_private, regular, restrict},
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const API_VERSION: u32 = 1;
/// Identifier of the interface that ships with the CLI and is refreshed by CLI updates.
pub const DEFAULT_ID: &str = "default";
const RECEIPT: &str = ".sqlx-ui-receipt.json";
#[derive(Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub cli_compat: String,
    pub entrypoint: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub description: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Selection {
    pub id: String,
    pub version: String,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    manifest: PluginManifest,
    files: BTreeMap<String, String>,
}
#[derive(Clone)]
pub struct Plugin {
    pub manifest: PluginManifest,
    root: PathBuf,
    files: BTreeMap<String, String>,
}
#[derive(Serialize)]
pub struct Listing {
    pub manifest: PluginManifest,
    pub active: bool,
}

fn directory(root: &Path) -> PathBuf {
    root.join("plugins/ui")
}
fn id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn validate(manifest: &PluginManifest) -> Result<()> {
    if manifest.schema_version != 1 || manifest.api_version != API_VERSION {
        bail!("UI plugin requires an unsupported manifest or API version");
    }
    if !id_valid(&manifest.id) {
        bail!("plugin ID must use lowercase letters, digits and hyphens (1-64 characters)");
    }
    if manifest.name.trim().is_empty() {
        bail!("UI plugin name is required");
    }
    semver::Version::parse(&manifest.version)?;
    if !semver::VersionReq::parse(&manifest.cli_compat)?
        .matches(&semver::Version::parse(env!("CARGO_PKG_VERSION"))?)
    {
        bail!("UI plugin requires CLI {}", manifest.cli_compat);
    }
    safe_join(Path::new("."), &manifest.entrypoint)?;
    if !manifest.entrypoint.ends_with(".html") {
        bail!("UI plugin entrypoint must be an HTML file");
    }
    for required in ["workspace", "datasource-setup", "query-results"] {
        if !manifest.capabilities.iter().any(|c| c == required) {
            bail!("UI plugin must support {required}");
        }
    }
    Ok(())
}
fn payload(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = files(root)?;
    result.remove(RECEIPT);
    for (name, digest) in &result {
        if digest.starts_with("symlink:") || name.split('/').any(|part| part.starts_with('.')) {
            bail!("UI plugins cannot contain hidden files or symbolic links");
        }
        if !matches!(
            Path::new(name).file_name().and_then(|s| s.to_str()),
            Some("LICENSE" | "NOTICE")
        ) && !matches!(
            Path::new(name).extension().and_then(|s| s.to_str()),
            Some(
                "html"
                    | "js"
                    | "mjs"
                    | "css"
                    | "json"
                    | "map"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "ico"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "txt"
                    | "md"
            )
        ) {
            bail!("UI plugin contains a non-web asset: {name}");
        }
    }
    Ok(result)
}
pub fn install(root: &Path, source: &Path) -> Result<PluginManifest> {
    let source = source.canonicalize()?;
    let manifest: PluginManifest =
        serde_json::from_slice(&fs::read(source.join("ui-plugin.json"))?)
            .context("UI package must contain ui-plugin.json")?;
    validate(&manifest)?;
    let original = payload(&source)?;
    let html = fs::read_to_string(safe_join(&source, &manifest.entrypoint)?)?;
    if !html.contains("__SQLX_UI_BASE__") {
        bail!("plugin HTML must use __SQLX_UI_BASE__ as its base URL");
    }
    let parent = directory(root);
    fs::create_dir_all(&parent)?;
    restrict(&parent, true)?;
    let lock = open_private(&parent.join("registry.lock"))?;
    lock.lock_exclusive()?;
    let versions = parent.join(&manifest.id);
    fs::create_dir_all(&versions)?;
    let target = versions.join(&manifest.version);
    if target.exists() {
        let installed = load(root, &manifest.id, &manifest.version)?;
        if installed.files != original {
            bail!(
                "this plugin version already exists with different content; publish a new version"
            );
        }
        return Ok(manifest);
    }
    let stage = tempfile::tempdir_in(&versions)?;
    for name in original.keys() {
        let target = safe_join(stage.path(), name)?;
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(safe_join(&source, name)?, target)?;
    }
    if payload(stage.path())? != original {
        bail!("plugin files changed during installation");
    }
    atomic_write(
        &stage.path().join(RECEIPT),
        &serde_json::to_vec(&Receipt {
            manifest: manifest.clone(),
            files: original,
        })?,
    )?;
    fs::rename(stage.path(), &target)?;
    Ok(manifest)
}
pub fn install_url(root: &Path, url: &str, sha256: &str) -> Result<PluginManifest> {
    let parent = root.join("plugin-downloads");
    let unpacked = components::download_plugin(url, sha256, &parent)?;
    install(root, unpacked.path())
}
pub fn load(root: &Path, id: &str, version: &str) -> Result<Plugin> {
    if !id_valid(id) {
        bail!("invalid plugin ID");
    }
    semver::Version::parse(version)?;
    let path = directory(root).join(id).join(version);
    let receipt_path = path.join(RECEIPT);
    regular(&receipt_path)?;
    let receipt: Receipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
    validate(&receipt.manifest)?;
    if receipt.manifest.id != id
        || receipt.manifest.version != version
        || payload(&path)? != receipt.files
    {
        bail!("UI plugin integrity verification failed");
    }
    Ok(Plugin {
        manifest: receipt.manifest,
        root: path.canonicalize()?,
        files: receipt.files,
    })
}
pub fn selected(root: &Path) -> Result<Option<Selection>> {
    let path = directory(root).join("active.json");
    if !path.exists() {
        return Ok(None);
    }
    regular(&path)?;
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}
pub fn active(root: &Path) -> Result<Plugin> {
    let selected = selected(root)?.context("no active UI plugin; start the UI through the CLI")?;
    load(root, &selected.id, &selected.version)
}
pub fn activate(root: &Path, id: &str, version: Option<&str>) -> Result<PluginManifest> {
    let parent = directory(root);
    fs::create_dir_all(&parent)?;
    let lock = open_private(&parent.join("registry.lock"))?;
    lock.lock_exclusive()?;
    let version = if let Some(version) = version {
        version.to_owned()
    } else {
        list(root)?
            .into_iter()
            .filter(|p| p.manifest.id == id)
            .max_by_key(|p| semver::Version::parse(&p.manifest.version).unwrap())
            .context("UI plugin is not installed")?
            .manifest
            .version
    };
    let plugin = load(root, id, &version)?;
    atomic_write(
        &parent.join("active.json"),
        &serde_json::to_vec(&Selection {
            id: id.into(),
            version,
        })?,
    )?;
    Ok(plugin.manifest)
}
pub fn list(root: &Path) -> Result<Vec<Listing>> {
    let parent = directory(root);
    if !parent.exists() {
        return Ok(vec![]);
    }
    let selected = selected(root)?;
    let mut entries = vec![];
    for id in fs::read_dir(&parent)? {
        let id = id?;
        if !id.file_type()?.is_dir() {
            continue;
        }
        let name = id.file_name().to_string_lossy().into_owned();
        if !id_valid(&name) {
            continue;
        }
        for version in fs::read_dir(id.path())? {
            let version = version?;
            if !version.file_type()?.is_dir() {
                continue;
            }
            let value = version.file_name().to_string_lossy().into_owned();
            if semver::Version::parse(&value).is_err() {
                continue;
            }
            let plugin = load(root, &name, &value)?;
            let active = selected
                .as_ref()
                .is_some_and(|s| s.id == name && s.version == value);
            entries.push(Listing {
                manifest: plugin.manifest,
                active,
            });
        }
    }
    entries.sort_by(|a, b| {
        a.manifest
            .id
            .cmp(&b.manifest.id)
            .then(a.manifest.version.cmp(&b.manifest.version))
    });
    Ok(entries)
}
pub fn remove(root: &Path, id: &str, version: &str) -> Result<()> {
    let parent = directory(root);
    let lock = open_private(&parent.join("registry.lock"))?;
    lock.lock_exclusive()?;
    if selected(root)?.is_some_and(|s| s.id == id && s.version == version) {
        bail!("switch to another UI plugin before removing the active version");
    }
    let plugin = load(root, id, version)?;
    fs::remove_dir_all(plugin.root)?;
    Ok(())
}
/// Install and select the interface that ships with this CLI.
///
/// A plugin the user installed always keeps the version they selected. `refresh` moves an
/// installed default interface to the version of the release manifest; without it only the cached
/// manifest is consulted, so a locally built interface never requires network access.
pub fn ensure_default(manager: &Components, refresh: bool) -> Result<()> {
    let current = selected(&manager.root)?;
    if current.as_ref().is_some_and(|s| s.id != DEFAULT_ID) {
        active(&manager.root)?;
        return Ok(());
    }
    let m = match &current {
        // A locally built interface is used as installed and never requires the release manifest.
        Some(_) if !refresh => manager.cached_manifest(),
        // An installed interface keeps working while the release manifest is unreachable.
        Some(_) => manager.manifest(false).ok(),
        None => Some(manager.manifest(false)?),
    };
    let Some(m) = m else {
        active(&manager.root)?;
        return Ok(());
    };
    let asset = manager
        .asset(&m, "ui-default", "any")
        .context("the release does not provide the default UI plugin")?;
    let installed = current
        .as_ref()
        .map(|s| semver::Version::parse(&s.version))
        .transpose()
        .context("the selected UI plugin has an invalid version")?;
    // Only a newer release replaces the installed default, so a manifest from another release
    // never downgrades an interface that is already current.
    let released = semver::Version::parse(&asset.version).context("invalid component version")?;
    if installed.is_some_and(|v| v >= released) {
        active(&manager.root)?;
        return Ok(());
    }
    let entry = manager.ensure("ui-default", "any", asset)?;
    let manifest = install(&manager.root, entry.parent().unwrap())?;
    let parent = directory(&manager.root);
    let lock = open_private(&parent.join("registry.lock"))?;
    lock.lock_exclusive()?;
    if selected(&manager.root)?.is_none_or(|s| s.id == manifest.id) {
        load(&manager.root, &manifest.id, &manifest.version)?;
        atomic_write(
            &parent.join("active.json"),
            &serde_json::to_vec(&Selection {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
            })?,
        )?;
    }
    Ok(())
}
impl Plugin {
    pub fn base(&self) -> String {
        format!("/_ui/{}/{}", self.manifest.id, self.manifest.version)
    }
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let expected = self.files.get(path).context("plugin asset not found")?;
        let file = safe_join(&self.root, path)?.canonicalize()?;
        if !file.starts_with(&self.root) || !file.is_file() || hash(&file)? != *expected {
            bail!("plugin asset integrity check failed");
        }
        Ok(fs::read(file)?)
    }
    pub fn html(&self) -> Result<String> {
        Ok(String::from_utf8(self.read(&self.manifest.entrypoint)?)?
            .replace("__SQLX_UI_BASE__", &self.base()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(root: &Path) -> PathBuf {
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("index.html"),
            "<html><head><base href=\"__SQLX_UI_BASE__/\"></head><body>Plugin</body></html>",
        )
        .unwrap();
        fs::write(
            source.join("ui-plugin.json"),
            serde_json::to_vec(&PluginManifest {
                schema_version: 1,
                id: "test-ui".into(),
                name: "Test".into(),
                version: "0.1.0".into(),
                api_version: 1,
                cli_compat: ">=0.1.1, <0.2.0".into(),
                entrypoint: "index.html".into(),
                capabilities: vec![
                    "workspace".into(),
                    "datasource-setup".into(),
                    "query-results".into(),
                ],
                description: String::new(),
            })
            .unwrap(),
        )
        .unwrap();
        source
    }
    #[test]
    fn install_select_and_integrity_are_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        install(temp.path(), &source).unwrap();
        activate(temp.path(), "test-ui", None).unwrap();
        assert!(active(temp.path())
            .unwrap()
            .html()
            .unwrap()
            .contains("/_ui/test-ui/0.1.0/"));
        assert!(remove(temp.path(), "test-ui", "0.1.0").is_err());
        fs::write(
            directory(temp.path()).join("test-ui/0.1.0/index.html"),
            "modified",
        )
        .unwrap();
        assert!(active(temp.path()).is_err());
    }
}
