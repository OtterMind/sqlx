use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx_protocol::Connection;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct Datasource {
    pub id: String,
    pub name: String,
    pub connection: Connection,
}
impl Datasource {
    pub fn public(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).expect("serializable datasource");
        let object = value["connection"].as_object_mut().unwrap();
        object.remove("username");
        object.remove("password");
        // Driver properties can contain credentials or vendor-specific tokens.
        object.remove("properties");
        value
    }
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Identity {
    pub device_id: String,
    pub installation_id: String,
    pub identity_version: u32,
    pub identity_source: String,
    pub identity_scope: String,
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    nonce: String,
    ciphertext: String,
}
pub struct Store {
    pub root: PathBuf,
    pub identity: Identity,
    key: [u8; 32],
    _lock: File,
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).context("cannot create SQLX data directory")?;
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            bail!("SQLX data directory must not be a symlink");
        }
        restrict(&root, true)?;
        let lock = open_private(&root.join("state.lock"))?;
        lock.lock_exclusive()?;
        let key_path = root.join("master.key");
        let database_path = root.join("datasources.enc");
        let key = if key_path.exists() {
            regular(&key_path)?;
            restrict(&key_path, false)?;
            fs::read(&key_path)?
                .try_into()
                .map_err(|_| anyhow!("invalid master key; existing data was not changed"))?
        } else {
            if database_path.exists() {
                bail!("master key is missing; existing encrypted data was not changed");
            }
            let mut key = [0; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            atomic_write(&key_path, &key)?;
            key
        };
        let identity_path = root.join("identity.json");
        let previous: Option<Identity> = if identity_path.exists() {
            regular(&identity_path)?;
            Some(serde_json::from_slice(&fs::read(&identity_path)?)?)
        } else {
            None
        };
        if previous.as_ref().is_some_and(|p| p.identity_version != 1) {
            bail!("unsupported identity version");
        }
        let identity = identity(previous.as_ref());
        atomic_write(&identity_path, &serde_json::to_vec_pretty(&identity)?)?;
        let store = Self {
            root,
            identity,
            key,
            _lock: lock,
        };
        if !database_path.exists() {
            store.save(&[])?;
        } else {
            store.load()?;
        }
        Ok(store)
    }
    pub fn load(&self) -> Result<Vec<Datasource>> {
        let path = self.root.join("datasources.enc");
        regular(&path)?;
        let e: Envelope = serde_json::from_slice(&fs::read(path)?)
            .context("invalid encrypted datasource file")?;
        if e.version != 1 {
            bail!("unsupported encrypted storage version");
        }
        let nonce = STANDARD.decode(e.nonce)?;
        if nonce.len() != 12 {
            bail!("invalid encryption nonce");
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("256-bit key");
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                aes_gcm::aead::Payload {
                    msg: &STANDARD.decode(e.ciphertext)?,
                    aad: b"ottermind.sqlx.datasources.v1",
                },
            )
            .map_err(|_| {
                anyhow!("datasource authentication failed; key or ciphertext is invalid")
            })?;
        serde_json::from_slice(&plaintext).context("invalid datasource document")
    }
    pub fn save(&self, values: &[Datasource]) -> Result<()> {
        let mut nonce = [0; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("256-bit key");
        let plaintext = serde_json::to_vec(values)?;
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                aes_gcm::aead::Payload {
                    msg: &plaintext,
                    aad: b"ottermind.sqlx.datasources.v1",
                },
            )
            .map_err(|_| anyhow!("encryption failed"))?;
        atomic_write(
            &self.root.join("datasources.enc"),
            &serde_json::to_vec(&Envelope {
                version: 1,
                nonce: STANDARD.encode(nonce),
                ciphertext: STANDARD.encode(encrypted),
            })?,
        )
    }
    pub fn find(&self, id: &str) -> Result<Datasource> {
        let values = self.load()?;
        values
            .into_iter()
            .find(|v| v.id == id || v.name == id)
            .ok_or_else(|| anyhow!("datasource not found"))
    }
}
pub fn regular(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("expected regular file: {}", path.display());
    }
    Ok(())
}
pub fn open_private(path: &Path) -> Result<File> {
    if path.exists() {
        regular(path)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let f = options.open(path)?;
    restrict(path, false)?;
    Ok(f)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        regular(path)?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().context("missing parent")?)?;
    restrict(temp.path(), false)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn restrict(path: &Path, directory: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )?;
    }
    #[cfg(windows)]
    {
        let user = Command::new("whoami")
            .output()
            .context("cannot determine current Windows user")?;
        if !user.status.success() {
            bail!("cannot determine current Windows user");
        }
        let grant = format!(
            "{}:{}F",
            String::from_utf8(user.stdout)?.trim(),
            if directory { "(OI)(CI)" } else { "" }
        );
        let result = Command::new("icacls")
            .arg(path)
            .args(["/inheritance:r", "/grant:r", &grant])
            .output()?;
        if !result.status.success() {
            bail!("cannot restrict SQLX file permissions");
        }
    }
    Ok(())
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn output(program: &str, args: &[&str]) -> Option<String> {
    let result = Command::new(program).args(args).output().ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).trim().to_string())
}
fn source(name: &str) -> Option<String> {
    let value = match name {
        #[cfg(target_os = "macos")]
        "ioplatformuuid" => {
            output("ioreg", &["-rd1", "-c", "IOPlatformExpertDevice"]).and_then(|s| {
                s.lines()
                    .find(|l| l.contains("\"IOPlatformUUID\""))
                    .and_then(|l| l.split('=').nth(1))
                    .map(|v| v.trim().trim_matches('"').to_string())
            })
        }
        #[cfg(target_os = "windows")]
        "smbios" => output(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystemProduct).UUID",
            ],
        ),
        #[cfg(target_os = "windows")]
        "machineguid" => output(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Cryptography').MachineGuid",
            ],
        ),
        #[cfg(target_os = "linux")]
        "dmi" => fs::read_to_string("/sys/class/dmi/id/product_uuid").ok(),
        #[cfg(target_os = "linux")]
        "machine-id" => fs::read_to_string("/etc/machine-id").ok(),
        _ => None,
    }?;
    let normalized = value.trim().to_ascii_lowercase();
    let compact: String = normalized.chars().filter(|c| *c != '-').collect();
    if compact.len() < 16 || compact.chars().all(|c| c == '0') || compact.chars().all(|c| c == 'f')
    {
        None
    } else {
        Some(normalized)
    }
}
fn identity(previous: Option<&Identity>) -> Identity {
    let candidates: &[(&str, &str)] = if cfg!(target_os = "macos") {
        &[("ioplatformuuid", "hardware")]
    } else if cfg!(windows) {
        &[("smbios", "hardware"), ("machineguid", "os")]
    } else {
        &[("dmi", "hardware"), ("machine-id", "os")]
    };
    let same_source = previous.filter(|old| {
        old.identity_source == "random"
            || candidates
                .iter()
                .any(|(name, _)| *name == old.identity_source)
    });
    let resolved = if let Some(old) = same_source {
        source(&old.identity_source)
            .map(|v| (old.identity_source.clone(), old.identity_scope.clone(), v))
    } else {
        candidates
            .iter()
            .find_map(|(name, scope)| source(name).map(|v| ((*name).into(), (*scope).into(), v)))
    };
    let Some((identity_source, identity_scope, raw)) = resolved else {
        return same_source.cloned().unwrap_or_else(|| Identity {
            device_id: Uuid::new_v4().to_string(),
            installation_id: Uuid::new_v4().to_string(),
            identity_version: 1,
            identity_source: "random".into(),
            identity_scope: "installation".into(),
        });
    };
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(b"ottermind.sqlx.device.v1").expect("valid HMAC key");
    mac.update(format!("{}\0{}\0{}", std::env::consts::OS, identity_source, raw).as_bytes());
    let device_id = hex::encode(mac.finalize().into_bytes());
    let installation_id = previous
        .filter(|old| old.device_id == device_id)
        .map(|old| old.installation_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    Identity {
        device_id,
        installation_id,
        identity_version: 1,
        identity_source,
        identity_scope,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encryption_authenticates_and_init_preserves_key() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(root.clone()).unwrap();
        let key = fs::read(root.join("master.key")).unwrap();
        let id = store.identity.installation_id.clone();
        drop(store);
        let store = Store::open(root.clone()).unwrap();
        assert_eq!(key, fs::read(root.join("master.key")).unwrap());
        assert_eq!(id, store.identity.installation_id);
        let mut e: Envelope =
            serde_json::from_slice(&fs::read(root.join("datasources.enc")).unwrap()).unwrap();
        let mut ciphertext = STANDARD.decode(&e.ciphertext).unwrap();
        ciphertext[0] ^= 1;
        e.ciphertext = STANDARD.encode(ciphertext);
        fs::write(
            root.join("datasources.enc"),
            serde_json::to_vec(&e).unwrap(),
        )
        .unwrap();
        assert!(store.load().is_err());
    }
    #[test]
    fn missing_key_never_replaces_existing_ciphertext() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        drop(Store::open(root.clone()).unwrap());
        let before = fs::read(root.join("datasources.enc")).unwrap();
        fs::remove_file(root.join("master.key")).unwrap();
        assert!(Store::open(root.clone()).is_err());
        assert!(!root.join("master.key").exists());
        assert_eq!(before, fs::read(root.join("datasources.enc")).unwrap());
    }
}
