//! Anonymous usage reporting attached to update checks.
//!
//! The desktop app reports one usage event per update check, and the CLI reports the same event to the
//! same Umami site so both products land in one dataset. They stay separable: the event carries
//! {@code product=SQLX} and the Umami tag {@code sqlx}, while the desktop reports its own product
//! label.
//!
//! Reporting is best effort: it never delays an update check by more than the request timeout and never
//! fails a command. A check that does not run reports nothing.

use chrono::{Datelike, Local, Timelike};
use reqwest::blocking::Client;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Umami site shared with the desktop application.
pub const ENDPOINT: &str = "https://um.ottermind.ai/api/send";
pub const WEBSITE_ID: &str = "51e9aab7-a6cd-44fb-8aa3-1db200cfaec7";
/// The CLI reports under its own host so the desktop segments that filter by hostname stay clean.
pub const HOSTNAME: &str = "sqlx-cli";
pub const PAGE_URL: &str = "/update-check";
pub const EVENT_NAME: &str = "update_check";
/// Product label that separates CLI usage from the desktop products.
pub const PRODUCT: &str = "SQLX";

/// The reporting request must never delay an update check.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
/// Same salt as the desktop reporter, so one machine reports one device id across products.
const DEVICE_SALT: &str = "chat2db-device-v1\n";
const MACHINE_PREFIX: &str = "d1_";
const RANDOM_PREFIX: &str = "r1_";
const DEVICE_FILE: &str = "device_id.json";
const CACHE_HEADER: &str = "x-umami-cache";

/// Why the update check ran, mirroring the trigger the desktop reports.
pub fn trigger(background: bool) -> &'static str {
    if background {
        "scheduled"
    } else {
        "manual"
    }
}

/// Outcome of a check record, in the desktop vocabulary so both can be charted together.
pub fn result(status: &str) -> &'static str {
    match status {
        "update_available" => "available",
        "check_failed" => "check_failed",
        _ => "no_update",
    }
}

/// Reports one update check. Every failure is swallowed: reporting must not change the command.
pub fn report_check(state_root: &Path, trigger: &str, result: &str, latest_version: Option<&str>) {
    let _ = send(state_root, trigger, result, latest_version);
}

fn send(
    state_root: &Path,
    trigger: &str,
    result: &str,
    latest_version: Option<&str>,
) -> Result<(), reqwest::Error> {
    let platform = platform();
    let language = language();
    let mut data = Map::new();
    data.insert("product".into(), json!(PRODUCT));
    data.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
    data.insert("channel".into(), json!("STABLE"));
    data.insert("platform".into(), json!(platform));
    data.insert("arch".into(), json!(arch()));
    data.insert("osVersion".into(), json!(os_version(platform)));
    data.insert("locale".into(), json!(language.tag));
    data.insert("timezone".into(), json!(timezone()));
    if let Some(region) = &language.region {
        data.insert("region".into(), json!(region));
    }
    let now = Local::now();
    data.insert(
        "activityDate".into(),
        json!(format!(
            "{:04}-{:02}-{:02}",
            now.year(),
            now.month(),
            now.day()
        )),
    );
    data.insert("activityHour".into(), json!(now.hour()));
    data.insert("trigger".into(), json!(trigger));
    data.insert("result".into(), json!(result));
    if let Some(version) = latest_version.filter(|value| !value.is_empty()) {
        data.insert("latestVersion".into(), json!(version));
    }

    let client = Client::builder()
        .user_agent(user_agent(platform))
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let device_id = device_id(state_root);
    let tag = PRODUCT.to_lowercase();
    let bodies = [
        identify_body(&device_id, &language.tag, &data),
        event_body(&language.tag, &tag, &data, false),
        event_body(&language.tag, &tag, &data, true),
    ];
    let mut cache = String::new();
    for body in bodies {
        let mut request = client
            .post(ENDPOINT)
            .header("Content-Type", "application/json")
            .body(body);
        if !cache.is_empty() {
            request = request.header(CACHE_HEADER, cache.clone());
        }
        let response = request.send()?;
        if let Ok(text) = response.text() {
            cache = cache_from_response(&text);
        }
    }
    Ok(())
}

/// The desktop reporter derives the device from the machine identifier; the CLI does the same so one
/// machine is one device, and falls back to a stored random id when no identifier can be read.
fn device_id(state_root: &Path) -> String {
    let file = state_root.join(DEVICE_FILE);
    if let Some(known) = read_device_id(&file) {
        return known;
    }
    let resolved = match read_machine_id() {
        Some(machine) => derive(&machine),
        None => random_id(),
    };
    let _ = write_device_id(&file, &resolved);
    resolved
}

fn read_device_id(file: &Path) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let device = value.get("device_id")?.as_str()?.trim().to_owned();
    (!device.is_empty()).then_some(device)
}

fn write_device_id(file: &Path, device: &str) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, serde_json::to_vec(&json!({"device_id": device}))?)?;
    restrict(file);
    Ok(())
}

#[cfg(unix)]
fn restrict(file: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(file, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_file: &Path) {}

/// One machine, one device id: the same derivation and salt as the desktop reporter.
fn derive(machine_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(DEVICE_SALT.as_bytes());
    digest.update(machine_id.trim().to_lowercase().as_bytes());
    format!("{MACHINE_PREFIX}{}", hex::encode(digest.finalize()))
}

fn random_id() -> String {
    format!("{RANDOM_PREFIX}{}", uuid::Uuid::new_v4().simple())
}

/// Machine identifier of the operating system, or {@code None} when it cannot be read.
fn read_machine_id() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = command("ioreg", &["-rd1", "-c", "IOPlatformExpertDevice"])?;
        return quoted_value(&output, "IOPlatformUUID");
    }
    #[cfg(target_os = "windows")]
    {
        let output = command(
            "reg",
            &[
                "query",
                "HKLM\\SOFTWARE\\Microsoft\\Cryptography",
                "/v",
                "MachineGuid",
            ],
        )?;
        return output.split_whitespace().last().map(str::to_owned);
    }
    #[cfg(target_os = "linux")]
    {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(text) = fs::read_to_string(path) {
                let value = text.trim();
                if !value.is_empty() {
                    return Some(value.to_owned());
                }
            }
        }
        return None;
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn command(program: &str, arguments: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let output = child.wait_with_output().ok()?;
    String::from_utf8(output.stdout).ok()
}

/// Value of a {@code "KEY" = "value"} pair, as printed by the macOS platform expert device.
#[cfg(target_os = "macos")]
fn quoted_value(output: &str, key: &str) -> Option<String> {
    let start = output.find(&format!("\"{key}\""))?;
    let rest = &output[start..];
    let value = rest.split('=').nth(1)?;
    let value = value.trim().trim_matches('"').trim();
    (!value.is_empty()).then(|| value.to_owned())
}

struct Language {
    tag: String,
    region: Option<String>,
}

/// Language tag from the environment, normalized the way the desktop reports it ({@code zh-CN}).
fn language() -> Language {
    let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| env::var(name).ok())
        .unwrap_or_default();
    language_from(&raw)
}

fn language_from(raw: &str) -> Language {
    let value = raw.split('.').next().unwrap_or_default().trim();
    let value = value.split('@').next().unwrap_or_default().trim();
    let mut parts = value.split(['_', '-']);
    let language = parts.next().unwrap_or_default().to_lowercase();
    let region = parts.next().map(str::to_uppercase).filter(|region| {
        region.len() == 2
            && region
                .chars()
                .all(|character| character.is_ascii_alphabetic())
    });
    let tag = match (&language[..], &region) {
        ("", _) => "en-US".to_owned(),
        (language, Some(region)) => format!("{language}-{region}"),
        (language, None) => language.to_owned(),
    };
    Language { tag, region }
}

fn timezone() -> String {
    if let Ok(value) = env::var("TZ") {
        let value = value.trim().trim_start_matches(':');
        if !value.is_empty() {
            return value.to_owned();
        }
    }
    if let Ok(link) = fs::read_link("/etc/localtime") {
        let text = link.to_string_lossy().to_string();
        if let Some((_, zone)) = text.split_once("zoneinfo/") {
            return zone.to_owned();
        }
    }
    let offset = Local::now().offset().local_minus_utc();
    let (sign, seconds) = if offset < 0 {
        ('-', -offset)
    } else {
        ('+', offset)
    };
    format!(
        "UTC{sign}{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

fn arch() -> &'static str {
    match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

/// Operating system release, the way the desktop reports it ({@code macOS 14.5}).
fn os_version(platform: &str) -> String {
    let version = os_release().unwrap_or_default();
    let version = version.trim();
    if version.is_empty() {
        platform.to_owned()
    } else {
        format!("{platform} {version}")
    }
}

fn os_release() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        return command("sw_vers", &["-productVersion"]).map(|value| value.trim().to_owned());
    }
    #[cfg(target_os = "linux")]
    {
        let text = fs::read_to_string("/etc/os-release").ok()?;
        return text
            .lines()
            .find_map(|line| line.strip_prefix("VERSION_ID="))
            .map(|value| value.trim().trim_matches('"').to_owned());
    }
    #[cfg(target_os = "windows")]
    {
        return env::var("OS").ok();
    }
    #[allow(unreachable_code)]
    None
}

/// The desktop reporter keeps a parseable platform prefix in its user agent so Umami derives the
/// environment; the product token changes to the CLI.
fn user_agent(platform: &str) -> String {
    let template = match platform {
        "Windows" => "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chat2DB-SQLX/1.0",
        "Linux" => "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chat2DB-SQLX/1.0",
        _ => "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chat2DB-SQLX/1.0",
    };
    template.to_owned()
}

/// The identify call creates the session, so it carries the device and the device facts.
fn identify_body(device_id: &str, language: &str, data: &Map<String, Value>) -> String {
    let mut identity = Map::new();
    identity.insert("id".into(), json!(device_id));
    for key in ["product", "version", "platform", "arch"] {
        if let Some(value) = data.get(key) {
            identity.insert(key.into(), value.clone());
        }
    }
    body("identify", language, None, &identity, false)
}

/// Umami derives pageviews, visitors and visits from page views, so a check reports one of these in
/// addition to its named event.
fn event_body(language: &str, tag: &str, data: &Map<String, Value>, named: bool) -> String {
    body("event", language, Some(tag), data, named)
}

fn body(
    kind: &str,
    language: &str,
    tag: Option<&str>,
    data: &Map<String, Value>,
    named: bool,
) -> String {
    let mut payload = Map::new();
    payload.insert("website".into(), json!(WEBSITE_ID));
    payload.insert("hostname".into(), json!(HOSTNAME));
    payload.insert("url".into(), json!(PAGE_URL));
    if !language.is_empty() {
        payload.insert("language".into(), json!(language));
    }
    if let Some(tag) = tag {
        payload.insert("tag".into(), json!(tag));
    }
    if named {
        payload.insert("name".into(), json!(EVENT_NAME));
    }
    payload.insert("data".into(), Value::Object(data.clone()));
    json!({"type": kind, "payload": payload}).to_string()
}

/// Session token the server returns, or an empty string when the response has none.
fn cache_from_response(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("cache")?.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// State file used by tests: the device id lives next to the update state.
pub fn device_file(state_root: &Path) -> PathBuf {
    state_root.join(DEVICE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_desktop_vocabulary() {
        assert_eq!(trigger(true), "scheduled");
        assert_eq!(trigger(false), "manual");
        assert_eq!(result("up_to_date"), "no_update");
        assert_eq!(result("update_available"), "available");
        assert_eq!(result("check_failed"), "check_failed");
        assert_eq!(result("checking"), "no_update");
    }

    #[test]
    fn device_id_is_stable_per_machine_and_per_product() {
        let derived = derive("A1B2C3");
        assert_eq!(derived, derive(" a1b2c3 "));
        assert!(derived.starts_with(MACHINE_PREFIX), "{derived}");
        assert_ne!(derived, derive("other-machine"));
        // The desktop reporter derives the same value from the same machine identifier.
        assert_eq!(
            derived,
            format!(
                "{MACHINE_PREFIX}{}",
                hex::encode(Sha256::digest(format!("{DEVICE_SALT}a1b2c3").as_bytes()))
            )
        );
    }

    #[test]
    fn device_id_is_persisted_once() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        assert!(read_device_id(&device_file(root)).is_none());
        let first = device_id(root);
        assert_eq!(first, device_id(root));
        assert_eq!(
            read_device_id(&device_file(root)).as_deref(),
            Some(&first[..])
        );
        fs::write(device_file(root), "{\"device_id\":\"kept\"}").unwrap();
        assert_eq!(device_id(root), "kept");
    }

    #[test]
    fn language_matches_the_desktop_tag() {
        assert_eq!(language_from("zh_CN.UTF-8").tag, "zh-CN");
        assert_eq!(language_from("zh_CN.UTF-8").region.as_deref(), Some("CN"));
        assert_eq!(language_from("en_US.UTF-8").tag, "en-US");
        assert_eq!(language_from("C").tag, "c");
        assert_eq!(language_from("").tag, "en-US");
        assert_eq!(language_from("de_DE@euro").tag, "de-DE");
    }

    #[test]
    fn payloads_carry_the_website_the_tag_and_the_product() {
        let mut data = Map::new();
        data.insert("product".into(), json!(PRODUCT));
        data.insert("version".into(), json!("0.1.18"));
        data.insert("platform".into(), json!("macOS"));
        data.insert("arch".into(), json!("arm64"));
        data.insert("result".into(), json!("available"));

        let identify: Value =
            serde_json::from_str(&identify_body("d1_abc", "zh-CN", &data)).unwrap();
        assert_eq!(identify["type"], "identify");
        assert_eq!(identify["payload"]["website"], WEBSITE_ID);
        assert_eq!(identify["payload"]["hostname"], HOSTNAME);
        assert_eq!(identify["payload"]["data"]["id"], "d1_abc");
        assert_eq!(identify["payload"]["data"]["product"], "SQLX");
        assert!(identify["payload"]["data"].get("result").is_none());

        let page_view: Value =
            serde_json::from_str(&event_body("zh-CN", "sqlx", &data, false)).unwrap();
        assert_eq!(page_view["type"], "event");
        assert_eq!(page_view["payload"]["tag"], "sqlx");
        assert!(page_view["payload"].get("name").is_none());
        assert_eq!(page_view["payload"]["data"]["result"], "available");

        let event: Value = serde_json::from_str(&event_body("zh-CN", "sqlx", &data, true)).unwrap();
        assert_eq!(event["payload"]["name"], EVENT_NAME);
        assert_eq!(event["payload"]["url"], PAGE_URL);
    }

    #[test]
    fn response_cache_is_read_when_present() {
        assert_eq!(cache_from_response("{\"cache\":\"token\"}"), "token");
        assert_eq!(cache_from_response("not json"), "");
        assert_eq!(cache_from_response("{}"), "");
    }
}
