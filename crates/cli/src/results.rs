use crate::storage::{atomic_write, open_private, restrict};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx_protocol::{Column, Event};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const RETENTION_SECONDS: u64 = 24 * 60 * 60;
/// Upper bound for the stored CLI results; the oldest ones are removed first.
pub const MAX_STORED_BYTES: u64 = 1 << 30;
/// Owner recorded in the metadata of a result the CLI or the page created.
pub const CLI_ORIGIN: &str = "cli";
pub const PAGE_ORIGIN: &str = "ui";
fn page_origin() -> String {
    PAGE_ORIGIN.into()
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Table {
    pub statement: usize,
    pub result: usize,
    pub columns: Vec<Column>,
    pub rows: u64,
    pub affected_rows: Option<String>,
    pub complete: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub result_id: String,
    pub datasource_id: String,
    pub datasource_name: String,
    pub statements: Vec<String>,
    pub created_at: u64,
    pub status: String,
    pub duration_ms: u64,
    pub tables: Vec<Table>,
    pub events: Vec<Value>,
    /// `cli` for results the command line stored, `ui` for page results.
    #[serde(default = "page_origin")]
    pub origin: String,
    #[serde(default)]
    pub snapshot: Option<String>,
    #[serde(default)]
    pub refresh: Option<Refresh>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Refresh {
    pub request_id: String,
    pub status: String,
    pub error: Option<String>,
}
pub struct ResultStore {
    pub metadata: Metadata,
    dir: PathBuf,
    metadata_path: PathBuf,
    active: Option<RowWriter>,
}
struct RowWriter {
    table: usize,
    rows: File,
    offsets: File,
    position: u64,
}
#[derive(Serialize)]
pub struct Page {
    pub rows: Vec<Vec<Value>>,
    pub offset: u64,
    pub next_offset: u64,
    pub total_rows: u64,
    pub complete: bool,
}

impl ResultStore {
    /// Create one result directory inside `parent`, which holds the results themselves.
    pub fn create(
        parent: &Path,
        id: &str,
        source_id: String,
        source_name: String,
        statements: Vec<String>,
        origin: &str,
    ) -> Result<Self> {
        uuid::Uuid::parse_str(id)?;
        let dir = parent.join(id);
        fs::create_dir_all(&dir)?;
        restrict(&dir, true)?;
        let store = Self {
            metadata: Metadata {
                result_id: id.into(),
                datasource_id: source_id,
                datasource_name: source_name,
                statements,
                created_at: now(),
                status: "queued".into(),
                duration_ms: 0,
                tables: vec![],
                events: vec![],
                origin: origin.into(),
                snapshot: None,
                refresh: None,
            },
            metadata_path: dir.join("metadata.json"),
            dir,
            active: None,
        };
        store.persist()?;
        Ok(store)
    }
    pub fn recover(path: &Path) -> Result<Self> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            bail!("invalid result directory");
        }
        let metadata_path = path.join("metadata.json");
        let metadata: Metadata = serde_json::from_slice(&fs::read(&metadata_path)?)?;
        let dir = if let Some(snapshot) = &metadata.snapshot {
            uuid::Uuid::parse_str(snapshot)?;
            let dir = path.join("snapshots").join(snapshot);
            if fs::symlink_metadata(&dir)?.file_type().is_symlink() {
                bail!("invalid snapshot directory");
            }
            dir
        } else {
            path.into()
        };
        let mut store = Self {
            metadata,
            dir,
            metadata_path,
            active: None,
        };
        if matches!(store.metadata.status.as_str(), "running" | "queued") {
            store.metadata.status = "interrupted".into();
            for table in &mut store.metadata.tables {
                let offsets = store
                    .dir
                    .join(format!("{}-{}.idx", table.statement, table.result));
                if let Ok(meta) = fs::metadata(offsets) {
                    table.rows = meta.len() / 8;
                }
            }
            store.metadata.events.push(serde_json::json!({"event":"error","code":"ui.interrupted","message":"The UI service stopped before execution was confirmed. This query was not rerun.","outcome":"unknown"}));
            store.persist()?;
        }
        if let Some(refresh) = &mut store.metadata.refresh {
            if refresh.status == "running" {
                refresh.status = "interrupted".into();
                refresh.error =
                    Some("Refresh was interrupted. The previous result is preserved.".into());
                store.persist()?;
                store.persist_refresh()?;
            }
        }
        Ok(store)
    }
    pub fn begin_refresh(&mut self, request_id: &str, source_name: String) -> Result<Self> {
        uuid::Uuid::parse_str(request_id)?;
        let dir = self
            .metadata_path
            .parent()
            .context("missing result directory")?
            .join("snapshots")
            .join(request_id);
        fs::create_dir_all(&dir)?;
        restrict(&dir, true)?;
        let mut metadata = self.metadata.clone();
        metadata.datasource_name = source_name;
        metadata.created_at = now();
        metadata.snapshot = Some(request_id.into());
        metadata.refresh = None;
        metadata.status = "queued".into();
        metadata.tables.clear();
        metadata.events.clear();
        metadata.duration_ms = 0;
        let next = Self {
            metadata,
            metadata_path: dir.join("metadata.json"),
            dir,
            active: None,
        };
        if let Err(error) = next.persist() {
            next.discard_refresh();
            return Err(error);
        }
        self.metadata.refresh = Some(Refresh {
            request_id: request_id.into(),
            status: "running".into(),
            error: None,
        });
        if let Err(error) = self.persist_refresh().and_then(|_| self.persist()) {
            let _ = self.fail_refresh(
                "failed",
                "Refresh could not start. The previous result is preserved.".into(),
            );
            next.discard_refresh();
            return Err(error);
        }
        Ok(next)
    }
    pub fn refresh_record(&self, request_id: &str) -> Result<Option<Refresh>> {
        uuid::Uuid::parse_str(request_id)?;
        if let Some(refresh) = &self.metadata.refresh {
            if refresh.request_id == request_id {
                return Ok(Some(refresh.clone()));
            }
        }
        let path = self
            .metadata_path
            .parent()
            .context("missing result directory")?
            .join("refreshes")
            .join(format!("{request_id}.json"));
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }
    fn persist_refresh(&self) -> Result<()> {
        if let Some(refresh) = &self.metadata.refresh {
            let dir = self
                .metadata_path
                .parent()
                .context("missing result directory")?
                .join("refreshes");
            fs::create_dir_all(&dir)?;
            restrict(&dir, true)?;
            atomic_write(
                &dir.join(format!("{}.json", refresh.request_id)),
                &serde_json::to_vec(refresh)?,
            )?;
        }
        Ok(())
    }
    pub fn fail_refresh(&mut self, status: &str, error: String) -> Result<()> {
        if let Some(refresh) = &mut self.metadata.refresh {
            refresh.status = status.into();
            refresh.error = Some(error);
        }
        self.persist()?;
        self.persist_refresh()
    }
    pub fn commit_refresh(&mut self, mut next: Self) -> Result<()> {
        next.metadata.refresh = self.metadata.refresh.clone();
        if let Some(refresh) = &mut next.metadata.refresh {
            refresh.status = "completed".into();
        }
        next.metadata_path = self.metadata_path.clone();
        // Publish the complete snapshot with one atomic metadata write. Failed
        // refreshes never replace the previous rows or their on-disk pointer.
        if let Err(error) = next.persist() {
            next.discard_refresh();
            return Err(error);
        }
        if next.persist_refresh().is_err() {
            eprintln!("Could not persist refresh history");
        }
        let previous = std::mem::replace(self, next);
        if previous.metadata.snapshot.is_some() {
            let _ = fs::remove_dir_all(previous.dir);
        }
        Ok(())
    }
    pub fn discard_refresh(self) {
        let _ = fs::remove_dir_all(self.dir);
    }
    pub fn record(&mut self, event: Event) -> Result<()> {
        match &event {
            Event::Ready { .. } | Event::Connected | Event::StatementStart { .. } => {
                self.metadata.status = "running".into();
            }
            Event::Columns {
                index,
                result,
                columns,
            } => {
                let rows = open_private(&self.dir.join(format!("{index}-{result}.jsonl")))?;
                let offsets = open_private(&self.dir.join(format!("{index}-{result}.idx")))?;
                self.metadata.tables.push(Table {
                    statement: *index,
                    result: *result,
                    columns: columns.clone(),
                    rows: 0,
                    affected_rows: None,
                    complete: false,
                });
                self.active = Some(RowWriter {
                    table: self.metadata.tables.len() - 1,
                    rows,
                    offsets,
                    position: 0,
                });
            }
            Event::Row { values, .. } => {
                let writer = self
                    .active
                    .as_mut()
                    .context("result row arrived without metadata")?;
                let encoded = serde_json::to_vec(values)?;
                writer.rows.write_all(&encoded)?;
                writer.rows.write_all(b"\n")?;
                writer.offsets.write_all(&writer.position.to_le_bytes())?;
                writer.position += encoded.len() as u64 + 1;
                self.metadata.tables[writer.table].rows += 1;
                return Ok(());
            }
            Event::ResultEnd {
                index,
                result,
                affected_rows,
                ..
            } => {
                let table = self
                    .metadata
                    .tables
                    .iter_mut()
                    .find(|t| t.statement == *index && t.result == *result)
                    .context("unknown result")?;
                table.affected_rows = affected_rows.clone();
                table.complete = true;
                self.active = None;
            }
            Event::Complete { .. } => {}
            Event::Error { .. } | Event::Skipped { .. } | Event::StatementEnd { .. } => {}
        }
        if !matches!(
            event,
            Event::Ready { .. }
                | Event::Connected
                | Event::Columns { .. }
                | Event::Complete { .. }
                | Event::ResultEnd { .. }
        ) {
            self.metadata.events.push(serde_json::to_value(event)?);
        }
        self.persist()
    }
    pub fn finish(
        &mut self,
        success: bool,
        cancelled: bool,
        duration_ms: u64,
        error: Option<String>,
    ) -> Result<()> {
        self.active = None;
        self.metadata.status = if cancelled {
            "cancelled"
        } else if success {
            "completed"
        } else {
            "failed"
        }
        .into();
        self.metadata.duration_ms = duration_ms;
        if let Some(message) = error {
            self.metadata.events.push(serde_json::json!({"event":"error","code":"ui.execution_failed","message":message,"outcome":"unknown"}));
        }
        self.persist()
    }
    pub fn page(&self, statement: usize, result: usize, offset: u64, limit: usize) -> Result<Page> {
        if !(1..=200).contains(&limit) {
            bail!("page limit must be between 1 and 200");
        }
        let table = self
            .metadata
            .tables
            .iter()
            .find(|t| t.statement == statement && t.result == result)
            .context("result set not found")?;
        let mut page = Page {
            rows: vec![],
            offset,
            next_offset: offset,
            total_rows: table.rows,
            complete: table.complete,
        };
        if offset >= table.rows {
            return Ok(page);
        }
        let mut offsets = File::open(self.dir.join(format!("{statement}-{result}.idx")))?;
        offsets.seek(SeekFrom::Start(
            offset.checked_mul(8).context("invalid offset")?,
        ))?;
        let mut bytes = [0; 8];
        offsets.read_exact(&mut bytes)?;
        let mut rows = BufReader::new(File::open(
            self.dir.join(format!("{statement}-{result}.jsonl")),
        )?);
        rows.seek(SeekFrom::Start(u64::from_le_bytes(bytes)))?;
        let mut page_bytes = 0;
        for _ in 0..limit.min((table.rows - offset) as usize) {
            let mut line = String::new();
            rows.read_line(&mut line)?;
            if line.is_empty() {
                bail!("result cache is incomplete");
            }
            if page_bytes + line.len() > 2 * 1024 * 1024 && !page.rows.is_empty() {
                break;
            }
            page_bytes += line.len();
            page.rows.push(serde_json::from_str(&line)?);
            page.next_offset += 1;
        }
        Ok(page)
    }
    pub fn expired(&self) -> bool {
        self.expired_after(RETENTION_SECONDS)
    }
    pub fn expired_after(&self, retention: u64) -> bool {
        self.metadata
            .refresh
            .as_ref()
            .is_none_or(|r| r.status != "running")
            && now().saturating_sub(self.metadata.created_at) > retention
    }
    /// Bytes this result occupies, used to bound the stored results.
    pub fn size(&self) -> u64 {
        fs::read_dir(&self.dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| entry.metadata().ok())
                    .map(|meta| meta.len())
                    .sum()
            })
            .unwrap_or(0)
    }
    pub fn remove(&self) -> Result<()> {
        fs::remove_dir_all(
            self.metadata_path
                .parent()
                .context("missing result directory")?,
        )?;
        Ok(())
    }
    fn persist(&self) -> Result<()> {
        atomic_write(&self.metadata_path, &serde_json::to_vec(&self.metadata)?)
    }
}
/// Read every result in a results directory, newest first. Unreadable entries are skipped.
pub fn stored(parent: &Path) -> Result<Vec<ResultStore>> {
    let mut found = Vec::new();
    if !parent.exists() {
        return Ok(found);
    }
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
            continue;
        }
        // Another process may remove a result while it is being read.
        if let Ok(store) = ResultStore::recover(&entry.path()) {
            found.push(store);
        }
    }
    found.sort_by_key(|store| std::cmp::Reverse(store.metadata.created_at));
    Ok(found)
}
/// Remove expired results of one origin and keep that origin under `max_bytes`, oldest first.
///
/// Results the page is still refreshing are never removed. Returns the removed ids.
pub fn prune(
    parent: &Path,
    origin: &str,
    retention: Option<u64>,
    max_bytes: u64,
) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for store in stored(parent)? {
        if store.metadata.origin == origin && retention.is_some_and(|s| store.expired_after(s)) {
            let id = store.metadata.result_id.clone();
            if store.remove().is_ok() {
                removed.push(id);
            }
            continue;
        }
        kept.push(store);
    }
    let mut total: u64 = kept
        .iter()
        .filter(|store| store.metadata.origin == origin)
        .map(ResultStore::size)
        .sum();
    // `stored` orders newest first, so iterate backwards to drop the oldest results first.
    for store in kept.iter().rev() {
        if total <= max_bytes {
            break;
        }
        if store.metadata.origin != origin {
            continue;
        }
        let size = store.size();
        if store.remove().is_ok() {
            total = total.saturating_sub(size);
            removed.push(store.metadata.result_id.clone());
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(store: &mut ResultStore, value: &str) {
        store
            .record(Event::Columns {
                index: 0,
                result: 0,
                columns: vec![Column {
                    name: "value".into(),
                    database_type: "int8".into(),
                    encoding: "string".into(),
                }],
            })
            .unwrap();
        store
            .record(Event::Row {
                index: 0,
                result: 0,
                values: vec![Value::String(value.into())],
            })
            .unwrap();
        store
            .record(Event::ResultEnd {
                index: 0,
                result: 0,
                rows: "1".into(),
                affected_rows: None,
            })
            .unwrap();
        store.finish(true, false, 1, None).unwrap();
    }
    #[test]
    fn failed_refresh_preparation_keeps_rows_and_does_not_leave_a_running_request() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let mut store = ResultStore::create(
            &root.path().join("results"),
            &id,
            "source".into(),
            "test".into(),
            vec!["SELECT 1".into()],
            PAGE_ORIGIN,
        )
        .unwrap();
        row(&mut store, "1");
        let blocked = root.path().join("results").join(&id).join("refreshes");
        fs::write(&blocked, "not a directory").unwrap();
        let request = uuid::Uuid::new_v4().to_string();
        assert!(store.begin_refresh(&request, "test".into()).is_err());
        assert_eq!(store.metadata.refresh.as_ref().unwrap().status, "failed");
        assert_eq!(store.page(0, 0, 0, 100).unwrap().rows[0][0], "1");
        assert!(!root
            .path()
            .join("results")
            .join(&id)
            .join("snapshots")
            .join(request)
            .exists());
        fs::remove_file(blocked).unwrap();
        let next = uuid::Uuid::new_v4().to_string();
        let mut staged = store.begin_refresh(&next, "test".into()).unwrap();
        row(&mut staged, "2");
        store.commit_refresh(staged).unwrap();
        assert_eq!(store.page(0, 0, 0, 100).unwrap().rows[0][0], "2");
    }
    #[test]
    fn refresh_publishes_complete_snapshots_and_preserves_previous_rows_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let mut store = ResultStore::create(
            &root.path().join("results"),
            &id,
            "source".into(),
            "test".into(),
            vec!["SELECT 1".into()],
            PAGE_ORIGIN,
        )
        .unwrap();
        row(&mut store, "1");
        let first = uuid::Uuid::new_v4().to_string();
        let mut staged = store.begin_refresh(&first, "test".into()).unwrap();
        row(&mut staged, "2");
        assert_eq!(store.page(0, 0, 0, 100).unwrap().rows[0][0], "1");
        store.commit_refresh(staged).unwrap();
        assert_eq!(store.page(0, 0, 0, 100).unwrap().rows[0][0], "2");
        let mut store = ResultStore::recover(&root.path().join("results").join(&id)).unwrap();
        assert_eq!(store.metadata.snapshot.as_deref(), Some(first.as_str()));
        assert_eq!(store.page(0, 0, 0, 100).unwrap().rows[0][0], "2");
        let failed = uuid::Uuid::new_v4().to_string();
        let staged = store.begin_refresh(&failed, "test".into()).unwrap();
        store
            .fail_refresh("failed", "Database unavailable".into())
            .unwrap();
        staged.discard_refresh();
        assert_eq!(
            store.refresh_record(&first).unwrap().unwrap().status,
            "completed"
        );
        assert_eq!(
            store.refresh_record(&failed).unwrap().unwrap().status,
            "failed"
        );
        let third = uuid::Uuid::new_v4().to_string();
        let mut staged = store.begin_refresh(&third, "test".into()).unwrap();
        row(&mut staged, "3");
        store.commit_refresh(staged).unwrap();
        assert!(!root
            .path()
            .join("results")
            .join(&id)
            .join("snapshots")
            .join(&first)
            .exists());
        let interrupted = uuid::Uuid::new_v4().to_string();
        let _staged = store.begin_refresh(&interrupted, "test".into()).unwrap();
        let store = ResultStore::recover(&root.path().join("results").join(&id)).unwrap();
        assert_eq!(
            store.refresh_record(&interrupted).unwrap().unwrap().status,
            "interrupted"
        );
        assert_eq!(store.page(0, 0, 0, 100).unwrap().rows[0][0], "3");
        assert_eq!(
            store.refresh_record(&first).unwrap().unwrap().status,
            "completed"
        );
    }
    #[test]
    fn pages_preserve_types_and_recovery_does_not_rerun() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let mut store = ResultStore::create(
            &root.path().join("results"),
            &id,
            "source".into(),
            "test".into(),
            vec!["select".into()],
            PAGE_ORIGIN,
        )
        .unwrap();
        store.record(Event::StatementStart { index: 0 }).unwrap();
        store
            .record(Event::Columns {
                index: 0,
                result: 0,
                columns: vec![Column {
                    name: "value".into(),
                    database_type: "decimal".into(),
                    encoding: "string".into(),
                }],
            })
            .unwrap();
        for i in 0..251 {
            store
                .record(Event::Row {
                    index: 0,
                    result: 0,
                    values: vec![Value::String(format!("{i}.4500"))],
                })
                .unwrap();
        }
        let page = store.page(0, 0, 200, 100).unwrap();
        assert_eq!(page.rows.len(), 51);
        assert_eq!(page.rows[0][0], "200.4500");
        drop(store);
        let recovered = ResultStore::recover(&root.path().join("results").join(id)).unwrap();
        assert_eq!(recovered.metadata.status, "interrupted");
        assert_eq!(recovered.page(0, 0, 250, 100).unwrap().rows.len(), 1);
    }
    fn stored_result(parent: &Path, origin: &str, age: u64) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut store = ResultStore::create(
            parent,
            &id,
            "source".into(),
            "test".into(),
            vec!["SELECT 1".into()],
            origin,
        )
        .unwrap();
        row(&mut store, "1");
        store.metadata.created_at = now().saturating_sub(age);
        store.persist().unwrap();
        id
    }
    #[test]
    fn prune_removes_expired_cli_results_and_keeps_page_results() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("results");
        let expired = stored_result(&parent, CLI_ORIGIN, RETENTION_SECONDS + 60);
        let fresh = stored_result(&parent, CLI_ORIGIN, 10);
        let page = stored_result(&parent, PAGE_ORIGIN, RETENTION_SECONDS + 60);
        let removed = prune(
            &parent,
            CLI_ORIGIN,
            Some(RETENTION_SECONDS),
            MAX_STORED_BYTES,
        )
        .unwrap();
        assert_eq!(removed, vec![expired.clone()]);
        let left: Vec<String> = stored(&parent)
            .unwrap()
            .iter()
            .map(|store| store.metadata.result_id.clone())
            .collect();
        assert!(left.contains(&fresh) && left.contains(&page), "{left:?}");
    }
    #[test]
    fn prune_keeps_the_page_results_when_only_cli_results_exceed_the_limit() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("results");
        let old = stored_result(&parent, CLI_ORIGIN, 600);
        let page = stored_result(&parent, PAGE_ORIGIN, 900);
        // A limit below a single result drops the oldest CLI result and no page result.
        let removed = prune(&parent, CLI_ORIGIN, None, 1).unwrap();
        assert_eq!(removed, vec![old.clone()]);
        let left: Vec<String> = stored(&parent)
            .unwrap()
            .iter()
            .map(|store| store.metadata.result_id.clone())
            .collect();
        assert_eq!(left, vec![page], "{left:?}");
    }
}
