use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx_core::storage::{atomic_write, open_private, restrict};
use sqlx_protocol::{Column, Event};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const RETENTION_SECONDS: u64 = 24 * 60 * 60;
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
}
pub struct ResultStore {
    pub metadata: Metadata,
    dir: PathBuf,
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
    pub fn create(
        root: &Path,
        id: &str,
        source_id: String,
        source_name: String,
        statements: Vec<String>,
    ) -> Result<Self> {
        uuid::Uuid::parse_str(id)?;
        let dir = root.join("results").join(id);
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
            },
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
        let metadata: Metadata = serde_json::from_slice(&fs::read(path.join("metadata.json"))?)?;
        let mut store = Self {
            metadata,
            dir: path.into(),
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
        Ok(store)
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
        now().saturating_sub(self.metadata.created_at) > RETENTION_SECONDS
    }
    pub fn remove(&self) -> Result<()> {
        fs::remove_dir_all(&self.dir)?;
        Ok(())
    }
    fn persist(&self) -> Result<()> {
        atomic_write(
            &self.dir.join("metadata.json"),
            &serde_json::to_vec(&self.metadata)?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_preserve_types_and_recovery_does_not_rerun() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let mut store = ResultStore::create(
            root.path(),
            &id,
            "source".into(),
            "test".into(),
            vec!["select".into()],
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
}
