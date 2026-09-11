//! Local UI bootstrap and control protocol. Database passwords never enter page URLs.
use crate::{
    components::{platform, Components},
    storage::{open_private, regular, Store},
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx_protocol::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const UI_PROTOCOL: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
pub struct UiState {
    pub protocol: u32,
    pub instance: String,
    pub origin: String,
    pub token: String,
    pub pid: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SetupRequest {
    pub name: String,
    pub source_id: Option<String>,
    pub connection: Connection,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ViewRequest {
    pub request_id: String,
    pub datasource: String,
    pub statements: Vec<String>,
}

pub struct UiClient {
    pub state: UiState,
    http: Client,
}

impl UiClient {
    pub fn existing(root: &Path) -> Result<Option<Self>> {
        let path = root.join("ui/state.json");
        if !path.exists() {
            return Ok(None);
        }
        regular(&path)?;
        let state: UiState = serde_json::from_slice(&fs::read(path)?)?;
        let url = reqwest::Url::parse(&state.origin)?;
        if state.protocol != UI_PROTOCOL
            || url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
        {
            bail!("invalid local UI state; refuse to contact an unrelated service");
        }
        let http = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()?;
        let response = match http
            .get(format!("{}/api/health", state.origin))
            .bearer_auth(&state.token)
            .send()
        {
            Ok(response) => response,
            Err(error) if error.is_connect() => return Ok(None),
            Err(_) => bail!("local UI is not responding; its process has not been replaced"),
        };
        let health: Value = response
            .error_for_status()
            .context("local UI authentication failed")?
            .json()?;
        if health["instance"].as_str() != Some(&state.instance) || health["protocol"] != UI_PROTOCOL
        {
            bail!("local UI instance mismatch");
        }
        Ok(Some(Self { state, http }))
    }

    pub fn start(root: PathBuf, manifest: String, local: Option<PathBuf>) -> Result<Self> {
        {
            Store::open(root.clone())?;
        }
        let root = root.canonicalize()?;
        let dir = root.join("ui");
        fs::create_dir_all(&dir)?;
        crate::storage::restrict(&dir, true)?;
        let lock = open_private(&dir.join("startup.lock"))?;
        lock.lock_exclusive()?;
        crate::plugins::ensure_default(&Components::new(root.clone(), manifest.clone()))?;
        if let Some(client) = Self::existing(&root)? {
            return Ok(client);
        }
        let binary = if let Some(ref local) = local {
            local.join(format!("sqlx-ui{}", std::env::consts::EXE_SUFFIX))
        } else {
            let manager = Components::new(root.clone(), manifest.clone());
            let m = manager.manifest(false)?;
            let p = platform()?;
            manager.ensure(
                "ui",
                &p,
                manager.asset(&m, "ui", &p).context(
                    "this release has no UI component; use a release with local UI support",
                )?,
            )?
        };
        let log = open_private(&dir.join("server.log"))?;
        log.set_len(0)?;
        let mut command = Command::new(binary);
        command
            .arg("--data-dir")
            .arg(&root)
            .arg("--manifest")
            .arg(manifest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log);
        if let Some(local) = local {
            command.arg("--worker-dir").arg(local.canonicalize()?);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x00000008 | 0x00000200);
        }
        let mut child = {
            // Windows inherits every inheritable handle, even when the child's standard
            // streams are redirected. Retaining an agent's pipe prevents it observing EOF.
            #[cfg(windows)]
            let _pipes = windows::StandardHandleInheritance::disable()?;
            command.spawn()
        }
        .context("could not start the local UI component")?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait()? {
                bail!("local UI exited during startup ({status}); check ui/server.log");
            }
            if let Some(client) = Self::existing(&root)? {
                return Ok(client);
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        bail!("local UI startup timed out")
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        self.response(
            self.http
                .get(format!("{}{path}", self.state.origin))
                .bearer_auth(&self.state.token)
                .send()?,
        )
    }
    pub fn post(&self, path: &str, body: &impl Serialize) -> Result<Value> {
        self.response(
            self.http
                .post(format!("{}{path}", self.state.origin))
                .bearer_auth(&self.state.token)
                .json(body)
                .send()?,
        )
    }
    fn response(&self, response: reqwest::blocking::Response) -> Result<Value> {
        let status = response.status();
        let value: Value = response.json()?;
        if !status.is_success() {
            bail!(
                "{}",
                value["error"]["message"]
                    .as_str()
                    .unwrap_or("local UI request failed")
            );
        }
        Ok(value)
    }
    pub fn page(&self, path: &str, open: bool) -> Result<String> {
        let value = self.post("/api/tickets", &json!({"path": path}))?;
        let url = value["url"]
            .as_str()
            .context("UI did not return a page URL")?
            .to_owned();
        if open {
            if let Err(error) = open_browser(&url) {
                eprintln!(
                    "Could not open the browser ({error}); open the returned page URL manually."
                );
            }
        }
        Ok(url)
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use windows_sys::Win32::{
        Foundation::{
            GetHandleInformation, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
            INVALID_HANDLE_VALUE,
        },
        System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
    };

    pub struct StandardHandleInheritance(Vec<HANDLE>);
    impl StandardHandleInheritance {
        pub fn disable() -> io::Result<Self> {
            let mut guard = Self(Vec::new());
            for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                // SAFETY: These are borrowed process standard handles. They remain open
                // while spawning; only their inheritance bit is changed and restored.
                unsafe {
                    let handle = GetStdHandle(stream);
                    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                        continue;
                    }
                    let mut flags = 0;
                    if GetHandleInformation(handle, &mut flags) == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if flags & HANDLE_FLAG_INHERIT != 0 {
                        if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) == 0 {
                            return Err(io::Error::last_os_error());
                        }
                        guard.0.push(handle);
                    }
                }
            }
            Ok(guard)
        }
    }
    impl Drop for StandardHandleInheritance {
        fn drop(&mut self) {
            for &handle in &self.0 {
                // SAFETY: Same borrowed handles as above; no ownership is transferred.
                unsafe {
                    SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
                }
            }
        }
    }
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status()?;
    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    #[cfg(target_os = "windows")]
    let status = Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .status()?;
    if !status.success() {
        bail!("system browser launcher failed");
    }
    Ok(())
}
