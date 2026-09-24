//! Batch import of datasources, for a tool that hands its own saved connections to SQLX.
//!
//! The document is versioned so another product can depend on it: each item carries a
//! datasource name and a complete connection object. Every item is validated on its own so
//! one unsupported engine reports its own reason instead of failing the whole document, and
//! the store is written once at the end so an import is never half applied.
use crate::{
    local_database_path,
    storage::{Datasource, Store},
    validate_name,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx_protocol::Connection;
use std::{
    collections::BTreeSet,
    io::{self, Read},
};

/// Version of the import document this build accepts.
const DOCUMENT_VERSION: u32 = 1;
/// Credentials travel through stdin, so the document stays small; anything larger is a mistake.
const MAX_DOCUMENT_BYTES: u64 = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    /// Add names that do not exist yet and update the ones that do, in place.
    Merge,
}
#[derive(Debug, Deserialize)]
struct Document {
    #[serde(default = "document_version")]
    version: u32,
    #[serde(default)]
    mode: Option<Mode>,
    datasources: Vec<Item>,
}
#[derive(Debug, Deserialize)]
struct Item {
    name: String,
    /// Converted per item so an unsupported engine is a reported skip, not a parse failure.
    connection: Value,
}
fn document_version() -> u32 {
    DOCUMENT_VERSION
}
#[derive(Debug, Default)]
struct Report {
    added: usize,
    updated: usize,
    unchanged: usize,
    total: usize,
    affected: Vec<Value>,
    skipped: Vec<Value>,
}
impl Report {
    fn changed(&self) -> bool {
        self.added > 0 || self.updated > 0
    }
    fn into_value(self, mode: Mode, dry_run: bool) -> Value {
        json!({
            "version":DOCUMENT_VERSION,
            "mode":mode,
            "dry_run":dry_run,
            "added":self.added,
            "updated":self.updated,
            "unchanged":self.unchanged,
            "total":self.total,
            "datasources":self.affected,
            "skipped":self.skipped,
        })
    }
}
/// Read the document from stdin, refusing input that cannot be a datasource document.
pub fn read_stdin() -> Result<String> {
    let mut input = String::new();
    io::stdin()
        .lock()
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_string(&mut input)
        .context("cannot read the import document from stdin")?;
    if input.len() as u64 > MAX_DOCUMENT_BYTES {
        bail!("the import document is larger than {MAX_DOCUMENT_BYTES} bytes");
    }
    Ok(input)
}
/// Apply one document to the store and return the report the CLI prints.
pub fn run(store: &Store, input: &str, dry_run: bool, strict: bool) -> Result<Value> {
    let (mode, ready, skipped) = prepare(input, strict)?;
    let mut sources = store.load()?;
    let report = apply(&mut sources, ready, skipped);
    if report.changed() && !dry_run {
        store.save(&sources)?;
    }
    Ok(report.into_value(mode, dry_run))
}
/// Validated entries that can be stored, as a name and its connection.
type Ready = Vec<(String, Connection)>;
/// Entries that cannot be imported, each with its reason code and detail.
type Skipped = Vec<Value>;
/// Parse and validate a document without touching the store.
fn prepare(input: &str, strict: bool) -> Result<(Mode, Ready, Skipped)> {
    let document: Document = serde_json::from_str(input).context(
        "the import document must be a JSON object with version, mode and a datasources array",
    )?;
    if document.version != DOCUMENT_VERSION {
        bail!(
            "unsupported import document version {}; this build accepts version {}",
            document.version,
            DOCUMENT_VERSION
        );
    }
    let mut ready = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &document.datasources {
        let name = item.name.trim().to_string();
        match connection(item, &name, &mut seen) {
            Ok(connection) => ready.push((name, connection)),
            Err((reason, detail)) => skipped.push(json!({
                "name":item.name,
                "reason":reason,
                "detail":detail,
            })),
        }
    }
    if strict && !skipped.is_empty() {
        bail!(
            "{} of {} datasources cannot be imported; nothing was changed (first: {} - {})",
            skipped.len(),
            document.datasources.len(),
            skipped[0]["name"].as_str().unwrap_or_default(),
            skipped[0]["detail"].as_str().unwrap_or_default()
        );
    }
    Ok((document.mode.unwrap_or(Mode::Merge), ready, skipped))
}
type Skip = (&'static str, String);
fn connection(item: &Item, name: &str, seen: &mut BTreeSet<String>) -> Result<Connection, Skip> {
    if let Err(error) = validate_name(name) {
        return Err(("invalid_name", format!("{error:#}")));
    }
    if !seen.insert(name.to_string()) {
        return Err((
            "duplicate_name",
            "the document lists this name more than once".into(),
        ));
    }
    let mut connection: Connection = serde_json::from_value(item.connection.clone())
        .map_err(|error| ("invalid_connection", error.to_string()))?;
    if connection.is_local() && !connection.database.trim().is_empty() {
        connection.database = local_database_path(&connection.database)
            .map_err(|error| ("invalid_connection", format!("{error:#}")))?;
    }
    connection
        .validate()
        .map_err(|error| ("invalid_connection", format!("{error:#}")))?;
    Ok(connection)
}
fn apply(
    sources: &mut Vec<Datasource>,
    ready: Vec<(String, Connection)>,
    skipped: Vec<Value>,
) -> Report {
    let mut report = Report {
        skipped,
        ..Report::default()
    };
    for (name, connection) in ready {
        match sources.iter_mut().find(|source| source.name == name) {
            Some(existing) => {
                if same_connection(&existing.connection, &connection) {
                    report.unchanged += 1;
                    continue;
                }
                existing.connection = connection;
                report.updated += 1;
                report.affected.push(existing.public());
            }
            None => {
                let source = Datasource {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    connection,
                };
                report.affected.push(source.public());
                sources.push(source);
                report.added += 1;
            }
        }
    }
    report.total = sources.len();
    report
}
fn same_connection(current: &Connection, requested: &Connection) -> bool {
    let current = serde_json::to_value(current);
    let requested = serde_json::to_value(requested);
    matches!((current, requested), (Ok(current), Ok(requested)) if current == requested)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn document(items: Value) -> String {
        json!({"version":1,"mode":"merge","datasources":items}).to_string()
    }
    fn connection() -> Value {
        json!({
            "database_type":"mysql","host":"127.0.0.1","port":3306,"database":"app",
            "username":"root","password":"secret","tls":"disable"
        })
    }
    fn item(name: &str) -> Value {
        json!({"name":name,"connection":connection()})
    }
    fn stored(name: &str) -> Datasource {
        Datasource {
            id: format!("id-{name}"),
            name: name.into(),
            connection: serde_json::from_value(connection()).unwrap(),
        }
    }
    fn temp_store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().join("data")).unwrap();
        (directory, store)
    }
    fn skipped(report: &Value) -> Vec<(String, String)> {
        report["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                (
                    item["name"].as_str().unwrap_or_default().to_string(),
                    item["reason"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }
    #[test]
    fn merge_adds_and_updates_by_name() {
        let mut sources = vec![stored("prod")];
        sources[0].connection.password = "old".into();
        let (mode, ready, skipped) =
            prepare(&document(json!([item("prod"), item("stage")])), false).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(mode, Mode::Merge);
        let report = apply(&mut sources, ready, skipped);
        assert_eq!(report.added, 1);
        assert_eq!(report.updated, 1);
        assert_eq!(report.unchanged, 0);
        assert_eq!(report.total, 2);
        assert_eq!(report.affected.len(), 2);
        // The stored record keeps its identity and the report never echoes credentials.
        assert_eq!(sources[0].id, "id-prod");
        assert_eq!(sources[0].connection.password, "secret");
        assert!(report
            .affected
            .iter()
            .all(|entry| entry["connection"]["password"].is_null()));
    }
    #[test]
    fn importing_the_same_document_twice_changes_nothing() {
        let (_temp, store) = temp_store();
        let input = document(json!([item("prod"), item("stage")]));
        let first = run(&store, &input, false, false).unwrap();
        assert_eq!(
            (first["added"].as_u64(), first["updated"].as_u64()),
            (Some(2), Some(0))
        );
        let second = run(&store, &input, false, false).unwrap();
        assert_eq!(
            (
                second["added"].as_u64(),
                second["updated"].as_u64(),
                second["unchanged"].as_u64()
            ),
            (Some(0), Some(0), Some(2))
        );
        assert_eq!(store.load().unwrap().len(), 2);
    }
    #[test]
    fn invalid_items_report_their_own_reason() {
        let input = document(json!([
            item("ok"),
            {"name":"","connection":connection()},
            {"name":"6f1d0f0e-8c58-4a5f-9d34-6a5a4f3b2c11","connection":connection()},
            item("twin"),
            item("twin"),
            {"name":"unknown","connection":{"database_type":"snowflake","host":"h","port":10000}},
            {"name":"nohost","connection":{"database_type":"postgresql","port":5432}},
        ]));
        let (_temp, store) = temp_store();
        let report = run(&store, &input, false, false).unwrap();
        assert_eq!(report["added"], 2);
        assert_eq!(
            skipped(&report),
            vec![
                ("".to_string(), "invalid_name".to_string()),
                (
                    "6f1d0f0e-8c58-4a5f-9d34-6a5a4f3b2c11".to_string(),
                    "invalid_name".to_string()
                ),
                ("twin".to_string(), "duplicate_name".to_string()),
                ("unknown".to_string(), "invalid_connection".to_string()),
                ("nohost".to_string(), "invalid_connection".to_string()),
            ]
        );
        assert_eq!(store.load().unwrap().len(), 2);
        assert!(report["skipped"][3]["detail"]
            .as_str()
            .unwrap()
            .contains("mysql"));
    }
    #[test]
    fn strict_refuses_the_whole_document() {
        let (_temp, store) = temp_store();
        let input = document(json!([
            item("ok"),
            {"name":"unknown","connection":{"database_type":"snowflake","host":"h","port":1}}
        ]));
        let error = run(&store, &input, false, true).unwrap_err().to_string();
        assert!(error.contains("cannot be imported"), "{error}");
        assert!(store.load().unwrap().is_empty());
    }
    #[test]
    fn dry_run_reports_without_storing() {
        let (_temp, store) = temp_store();
        let report = run(&store, &document(json!([item("prod")])), true, false).unwrap();
        assert_eq!(report["dry_run"], true);
        assert_eq!(report["added"], 1);
        assert!(store.load().unwrap().is_empty());
    }
    #[test]
    fn empty_document_succeeds_without_writing() {
        let (_temp, store) = temp_store();
        let report = run(&store, &document(json!([])), false, false).unwrap();
        assert_eq!(report["added"], 0);
        assert_eq!(report["total"], 0);
        assert_eq!(report["datasources"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn local_files_are_stored_as_absolute_paths() {
        let input = document(json!([{"name":"local","connection":{
            "database_type":"sqlite","host":"","database":"~/data/app.db","port":0
        }}]));
        let (_, ready, _) = prepare(&input, false).unwrap();
        assert_eq!(ready.len(), 1);
        assert!(std::path::Path::new(&ready[0].1.database).is_absolute());
        assert!(ready[0].1.database.ends_with("data/app.db"));
        let missing = document(json!([{"name":"local","connection":{
            "database_type":"sqlite","host":"","port":0
        }}]));
        let (_, ready, _) = prepare(&missing, false).unwrap();
        assert!(ready.is_empty());
    }
    #[test]
    fn malformed_documents_are_rejected() {
        let (_temp, store) = temp_store();
        for input in [
            "",
            "[]",
            "{}",
            r#"{"version":2,"datasources":[]}"#,
            r#"{"version":1,"mode":"replace","datasources":[]}"#,
        ] {
            assert!(
                run(&store, input, false, false).is_err(),
                "{input} was accepted"
            );
        }
    }
}
