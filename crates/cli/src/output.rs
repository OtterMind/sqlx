//! The result one command prints.
//!
//! The default mode folds the worker event stream into a table-shaped object that carries the
//! columns, a bounded preview and the row counts. A result set that does not fit the preview is
//! stored completely under `results/<id>/<statement>-<result>.jsonl` and the printed item points at
//! that file, so a large result never travels through standard output. `Mode::Events` prints the
//! raw worker stream instead, which is what scripts and the worker protocol expect.
use crate::{
    results::{ResultStore, CLI_ORIGIN},
    settings::{self, PREVIEW_BYTES},
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx_protocol::{Column, Event};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Table-shaped output with a bounded preview (default).
    Compact,
    /// The raw worker event stream.
    Events,
}
pub struct Options {
    pub mode: Mode,
    /// Print every row and store nothing, for a caller that cannot read the stored file.
    pub full: bool,
    pub preview_rows: u64,
    pub datasource_id: String,
    pub datasource_name: String,
    pub statements: Vec<String>,
    pub results_dir: PathBuf,
    /// Whether the result directory is the private default, which may not be a shared directory.
    pub results_owned: bool,
    /// Data directory used to check the owner of the default result directory.
    pub data_root: PathBuf,
}
/// The result set currently being printed.
struct Item {
    statement: usize,
    result: usize,
    columns: Vec<Column>,
    /// Whether the rows array is still open in the printed object.
    rows_open: bool,
    /// Whether this result set is being written to the store as well.
    stored: bool,
    /// Whether the preview of this result set is complete and later rows stay in the store.
    preview_closed: bool,
    /// Rows already printed, kept so a stored file stays complete when this set spills.
    pending: Vec<Vec<Value>>,
    printed_rows: u64,
    printed_bytes: usize,
    file: Option<PathBuf>,
}
pub struct Writer<W: Write> {
    output: W,
    options: Options,
    /// Compact: whether `{"results":[` was written; events: whether the envelope started.
    started: bool,
    first_item: bool,
    item: Option<Item>,
    /// The statement the worker reported last, used to attribute failures.
    statement: Option<usize>,
    /// Whether the active statement already produced an item.
    statement_item: bool,
    skipped: Vec<usize>,
    batch_error: Option<Value>,
    store: Option<ResultStore>,
    result_id: Option<String>,
    started_at: std::time::Instant,
    closed: bool,
}

impl<W: Write> Writer<W> {
    pub fn new(output: W, options: Options) -> Self {
        Self {
            output,
            options,
            started: false,
            first_item: true,
            item: None,
            statement: None,
            statement_item: false,
            skipped: Vec::new(),
            batch_error: None,
            store: None,
            result_id: None,
            started_at: std::time::Instant::now(),
            closed: false,
        }
    }
    /// Fold one worker event into the printed result.
    pub fn event(&mut self, event: Event) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        match self.options.mode {
            Mode::Events => self.write_event(event),
            Mode::Compact => self.write_compact(event),
        }
    }
    /// Close the printed object. Always leaves exactly one valid JSON value behind.
    pub fn finish(&mut self, success: bool) -> Result<()> {
        let success = success && self.batch_error.is_none();
        let elapsed = self.elapsed();
        if let Some(store) = self.store.as_mut() {
            store.finish(success, false, elapsed, None)?;
        }
        self.close(success)
    }
    /// Report a failure that happened outside the worker protocol, then close the object.
    pub fn failure(&mut self, code: &str, message: &str) -> Result<()> {
        let (index, outcome) = match self.statement {
            Some(index) => (Some(index), "unknown"),
            None => (None, "not_started"),
        };
        let skipped_from = index.map_or(0, |index| index + 1);
        match self.options.mode {
            Mode::Events => {
                self.write_event(Event::Error {
                    index,
                    code: code.into(),
                    message: message.into(),
                    outcome: outcome.into(),
                })?;
                for skipped in skipped_from..self.options.statements.len() {
                    self.write_event(Event::Skipped { index: skipped })?;
                }
            }
            Mode::Compact => {
                let error = json!({"code":code,"message":message,"outcome":outcome});
                match index {
                    Some(index) => self.attach_error(index, error)?,
                    None => self.batch_error = Some(error),
                }
                for skipped in skipped_from..self.options.statements.len() {
                    self.push_skipped(skipped);
                }
            }
        }
        let elapsed = self.elapsed();
        if let Some(store) = self.store.as_mut() {
            store.finish(false, false, elapsed, Some(message.to_owned()))?;
        }
        self.close(false)
    }
    fn elapsed(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
    fn close(&mut self, success: bool) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        match self.options.mode {
            Mode::Events => {
                if !self.started {
                    self.start_events()?;
                }
                writeln!(self.output, "],\"success\":{success}}}")?;
            }
            Mode::Compact => {
                self.close_open_rows()?;
                if self.started {
                    write!(self.output, "],")?;
                } else {
                    write!(self.output, "{{")?;
                }
                if !self.skipped.is_empty() {
                    self.skipped.sort_unstable();
                    write!(self.output, "\"skipped\":[")?;
                    for (position, index) in self.skipped.iter().enumerate() {
                        if position > 0 {
                            write!(self.output, ",")?;
                        }
                        write!(self.output, "{index}")?;
                    }
                    write!(self.output, "],")?;
                }
                if let Some(error) = &self.batch_error {
                    write!(self.output, "\"error\":")?;
                    serde_json::to_writer(&mut self.output, error)?;
                    write!(self.output, ",")?;
                }
                if let Some(id) = &self.result_id {
                    write!(self.output, "\"id\":")?;
                    serde_json::to_writer(&mut self.output, id)?;
                    write!(self.output, ",")?;
                }
                writeln!(self.output, "\"success\":{success}}}")?;
            }
        }
        self.output.flush()?;
        Ok(())
    }
    fn start_events(&mut self) -> Result<()> {
        write!(
            self.output,
            "{{\"protocol_version\":{},\"datasource_id\":{},\"events\":[",
            sqlx_protocol::VERSION,
            serde_json::to_string(&self.options.datasource_id)?
        )?;
        self.started = true;
        Ok(())
    }
    fn write_event(&mut self, event: Event) -> Result<()> {
        if !self.started {
            self.start_events()?;
        } else {
            write!(self.output, ",")?;
        }
        serde_json::to_writer(&mut self.output, &event)?;
        self.output.flush()?;
        Ok(())
    }
    fn write_compact(&mut self, event: Event) -> Result<()> {
        // The store keeps the lifecycle of a stored result, including its status.
        if self.store.is_some() {
            match &event {
                Event::Ready { .. } | Event::Connected | Event::Complete { .. } => {
                    self.record_to_store(event)?;
                    return Ok(());
                }
                Event::StatementStart { .. }
                | Event::StatementEnd { .. }
                | Event::Error { .. }
                | Event::Skipped { .. } => self.record_to_store(event.clone())?,
                _ => {}
            }
        }
        match event {
            Event::Ready { .. } | Event::Connected | Event::Complete { .. } => Ok(()),
            Event::StatementStart { index } => {
                self.statement = Some(index);
                self.statement_item = false;
                Ok(())
            }
            Event::Columns {
                index,
                result,
                columns,
            } => {
                if self.store.is_some() {
                    self.record_to_store(Event::Columns {
                        index,
                        result,
                        columns: columns.clone(),
                    })?;
                }
                if columns.is_empty() {
                    // A statement without a result set reports its count when it ends.
                    return Ok(());
                }
                self.open_item(index, result, columns)
            }
            Event::Row { values, .. } => self.write_row(values),
            Event::ResultEnd {
                index,
                result,
                rows,
                affected_rows,
            } => self.close_result(index, result, rows, affected_rows),
            Event::StatementEnd { index } => {
                if !self.statement_item && self.item.is_none() {
                    self.write_bare(index)?;
                }
                self.statement = None;
                self.statement_item = false;
                Ok(())
            }
            Event::Error {
                index,
                code,
                message,
                outcome,
            } => {
                let error = json!({"code":code,"message":message,"outcome":outcome});
                match index {
                    Some(index) => self.attach_error(index, error),
                    None => {
                        self.batch_error = Some(error);
                        Ok(())
                    }
                }
            }
            Event::Skipped { index } => {
                self.push_skipped(index);
                Ok(())
            }
        }
    }
    fn push_skipped(&mut self, index: usize) {
        if !self.skipped.contains(&index) {
            self.skipped.push(index);
        }
    }
    fn start_results(&mut self) -> Result<()> {
        if !self.started {
            write!(self.output, "{{\"results\":[")?;
            self.started = true;
        }
        Ok(())
    }
    fn separator(&mut self) -> Result<()> {
        if !self.first_item {
            write!(self.output, ",")?;
        }
        self.first_item = false;
        Ok(())
    }
    /// Open a result set that streams its rows.
    fn open_item(&mut self, statement: usize, result: usize, columns: Vec<Column>) -> Result<()> {
        self.close_open_rows()?;
        self.start_results()?;
        self.separator()?;
        write!(self.output, "{{\"stmt\":{statement}")?;
        if result > 0 {
            write!(self.output, ",\"set\":{result}")?;
        }
        write!(self.output, ",\"cols\":[")?;
        for (position, column) in columns.iter().enumerate() {
            if position > 0 {
                write!(self.output, ",")?;
            }
            serde_json::to_writer(&mut self.output, &column_json(column))?;
        }
        write!(self.output, "],\"rows\":[")?;
        self.item = Some(Item {
            statement,
            result,
            columns,
            rows_open: true,
            stored: self.store.is_some(),
            preview_closed: false,
            pending: Vec::new(),
            printed_rows: 0,
            printed_bytes: 0,
            file: None,
        });
        self.statement_item = true;
        Ok(())
    }
    /// Write a complete item that has no rows to stream.
    fn write_bare(&mut self, statement: usize) -> Result<()> {
        self.start_results()?;
        self.separator()?;
        write!(self.output, "{{\"stmt\":{statement}}}")?;
        self.statement_item = true;
        Ok(())
    }
    fn write_row(&mut self, values: Vec<Value>) -> Result<()> {
        let encoded = serde_json::to_vec(&values)?;
        let Some(item) = self.item.as_ref() else {
            return Ok(());
        };
        if !item.rows_open {
            return Ok(());
        }
        let (statement, result, stored, print) = (
            item.statement,
            item.result,
            item.stored,
            self.options.full
                || (!item.preview_closed
                    && item.printed_rows < self.options.preview_rows
                    && item.printed_bytes + encoded.len() <= PREVIEW_BYTES),
        );
        if print {
            if item.printed_rows > 0 {
                write!(self.output, ",")?;
            }
            self.output.write_all(&encoded)?;
            let item = self.item.as_mut().context("no result set to print into")?;
            item.printed_rows += 1;
            item.printed_bytes += encoded.len();
        } else {
            self.item
                .as_mut()
                .context("no result set to print into")?
                .preview_closed = true;
        }
        if self.options.full {
            return Ok(());
        }
        if stored {
            return self.record_to_store(Event::Row {
                index: statement,
                result,
                values,
            });
        }
        self.item
            .as_mut()
            .context("no result set to store")?
            .pending
            .push(values);
        if print {
            return Ok(());
        }
        self.spill()
    }
    /// Store the current result set, including the rows that were already printed.
    fn spill(&mut self) -> Result<()> {
        if self.options.full {
            // Nothing is stored in full mode, so there is no file to point at.
            return Ok(());
        }
        let (statement, result, columns, pending) = match self.item.as_mut() {
            Some(item) => (
                item.statement,
                item.result,
                item.columns.clone(),
                std::mem::take(&mut item.pending),
            ),
            None => return Ok(()),
        };
        let id = self.create_store()?;
        self.record_to_store(Event::Columns {
            index: statement,
            result,
            columns,
        })?;
        for values in pending {
            self.record_to_store(Event::Row {
                index: statement,
                result,
                values,
            })?;
        }
        let file = self
            .options
            .results_dir
            .join(&id)
            .join(format!("{statement}-{result}.jsonl"));
        let item = self.item.as_mut().context("no result set to store")?;
        item.stored = true;
        item.file = Some(file);
        Ok(())
    }
    fn close_result(
        &mut self,
        index: usize,
        result: usize,
        rows: String,
        affected_rows: Option<String>,
    ) -> Result<()> {
        let open = self
            .item
            .as_ref()
            .is_some_and(|item| item.statement == index && item.result == result);
        if !open {
            if self.store.is_some() {
                self.record_to_store(Event::ResultEnd {
                    index,
                    result,
                    rows: rows.clone(),
                    affected_rows: affected_rows.clone(),
                })?;
            }
            self.start_results()?;
            self.separator()?;
            write!(self.output, "{{\"stmt\":{index}")?;
            if result > 0 {
                write!(self.output, ",\"set\":{result}")?;
            }
            if let Some(affected) = &affected_rows {
                write!(self.output, ",\"affected\":")?;
                serde_json::to_writer(&mut self.output, affected)?;
            }
            write!(self.output, "}}")?;
            self.statement_item = true;
            return Ok(());
        }
        if self.item.as_ref().is_some_and(|item| item.stored) {
            self.record_to_store(Event::ResultEnd {
                index,
                result,
                rows: rows.clone(),
                affected_rows: affected_rows.clone(),
            })?;
        }
        let (stored, file, printed) = match self.item.as_ref() {
            Some(item) => (item.stored, item.file.clone(), item.printed_rows),
            None => (false, None, 0),
        };
        write!(self.output, "],\"count\":")?;
        serde_json::to_writer(&mut self.output, &rows)?;
        let total = rows.parse::<u64>().unwrap_or(0);
        if stored && printed < total {
            if let Some(file) = &file {
                write!(self.output, ",\"file\":")?;
                serde_json::to_writer(&mut self.output, &file.to_string_lossy().into_owned())?;
            }
        }
        if let Some(affected) = &affected_rows {
            write!(self.output, ",\"affected\":")?;
            serde_json::to_writer(&mut self.output, affected)?;
        }
        write!(self.output, "}}")?;
        self.item = None;
        Ok(())
    }
    /// Attach a statement error, closing whatever item is open for that statement.
    fn attach_error(&mut self, statement: usize, error: Value) -> Result<()> {
        if let Some(item) = self.item.as_ref() {
            if item.statement == statement && item.rows_open {
                // A stored result keeps whatever arrived before the failure, so point at it.
                let file = item.file.clone().filter(|_| item.stored);
                write!(self.output, "]")?;
                if let Some(file) = &file {
                    write!(self.output, ",\"file\":")?;
                    serde_json::to_writer(&mut self.output, &file.to_string_lossy().into_owned())?;
                }
                write!(self.output, ",\"error\":")?;
                serde_json::to_writer(&mut self.output, &error)?;
                write!(self.output, "}}")?;
                self.item = None;
                return Ok(());
            }
        }
        self.start_results()?;
        self.separator()?;
        write!(self.output, "{{\"stmt\":{statement},\"error\":")?;
        serde_json::to_writer(&mut self.output, &error)?;
        write!(self.output, "}}")?;
        self.statement_item = true;
        Ok(())
    }
    /// Close a rows array that never received a row count, so the object stays valid.
    fn close_open_rows(&mut self) -> Result<()> {
        if self.item.as_ref().is_some_and(|item| item.rows_open) {
            write!(self.output, "]}}")?;
            self.item = None;
        }
        Ok(())
    }
    fn create_store(&mut self) -> Result<String> {
        if let Some(id) = &self.result_id {
            return Ok(id.clone());
        }
        settings::ensure_results_dir(
            &self.options.results_dir,
            self.options.results_owned,
            &self.options.data_root,
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        let store = ResultStore::create(
            &self.options.results_dir,
            &id,
            self.options.datasource_id.clone(),
            self.options.datasource_name.clone(),
            self.options.statements.clone(),
            CLI_ORIGIN,
        )?;
        self.store = Some(store);
        self.result_id = Some(id.clone());
        Ok(id)
    }
    fn record_to_store(&mut self, event: Event) -> Result<()> {
        if let Some(store) = self.store.as_mut() {
            store.record(event)?;
        }
        Ok(())
    }
}
/// The compact column form: `[name, type]`, with the encoding only when it is not plain text.
pub fn column_json(column: &Column) -> Value {
    let mut value = vec![
        Value::String(column.name.clone()),
        Value::String(short_type(&column.database_type)),
    ];
    if column.encoding != "string" {
        value.push(Value::String(column.encoding.clone()));
    }
    Value::Array(value)
}
/// Driver type names shortened to what a caller has to know; MySQL reports protocol enum names.
pub fn short_type(database_type: &str) -> String {
    let name = database_type.to_ascii_lowercase();
    let Some(mysql) = name.strip_prefix("mysql_type_") else {
        return name;
    };
    match mysql {
        "tiny" => "tinyint",
        "short" => "smallint",
        "int24" => "mediumint",
        "long" => "int",
        "longlong" => "bigint",
        "float" => "float",
        "double" => "double",
        "decimal" | "newdecimal" => "decimal",
        "null" => "null",
        "timestamp" => "timestamp",
        "date" | "newdate" => "date",
        "time" => "time",
        "datetime" => "datetime",
        "year" => "year",
        "varchar" | "var_string" => "varchar",
        "string" => "char",
        "bit" => "bit",
        "json" => "json",
        "enum" => "enum",
        "set" => "set",
        "tiny_blob" | "medium_blob" | "long_blob" | "blob" => "blob",
        "geometry" => "geometry",
        other => other,
    }
    .to_owned()
}
/// Build the writer for one execution.
#[allow(clippy::too_many_arguments)]
pub fn writer_options(
    mode: Mode,
    full: bool,
    preview_rows: u64,
    datasource_id: &str,
    datasource_name: &str,
    statements: Vec<String>,
    results_dir: &Path,
    results_owned: bool,
    data_root: &Path,
) -> Options {
    Options {
        mode,
        full,
        preview_rows,
        datasource_id: datasource_id.to_owned(),
        datasource_name: datasource_name.to_owned(),
        statements,
        results_dir: results_dir.to_path_buf(),
        results_owned,
        data_root: data_root.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(mode: Mode, preview_rows: u64, dir: &Path) -> Options {
        super::writer_options(
            mode,
            false,
            preview_rows,
            "datasource",
            "dev",
            vec!["SELECT 1".into(), "SELECT 2".into(), "SELECT 3".into()],
            &dir.join("results"),
            false,
            dir,
        )
    }
    fn column(name: &str, database_type: &str) -> Column {
        Column {
            name: name.into(),
            database_type: database_type.into(),
            encoding: "string".into(),
        }
    }
    fn select(index: usize, rows: &[&str]) -> Vec<Event> {
        let mut events = vec![
            Event::Ready {
                protocol_version: sqlx_protocol::VERSION,
            },
            Event::Connected,
            Event::StatementStart { index },
            Event::Columns {
                index,
                result: 0,
                columns: vec![column("value", "MYSQL_TYPE_LONG")],
            },
        ];
        for row in rows {
            events.push(Event::Row {
                index,
                result: 0,
                values: vec![Value::String((*row).into())],
            });
        }
        events.push(Event::ResultEnd {
            index,
            result: 0,
            rows: rows.len().to_string(),
            affected_rows: None,
        });
        events.push(Event::StatementEnd { index });
        events
    }
    fn run(events: Vec<Event>, options: Options, success: bool) -> (String, Writer<Vec<u8>>) {
        let mut writer = Writer::new(Vec::new(), options);
        for event in events {
            writer.event(event).unwrap();
        }
        writer.finish(success).unwrap();
        let printed = String::from_utf8(writer.output.clone()).unwrap();
        (printed, writer)
    }
    #[test]
    fn a_small_result_is_printed_completely_and_not_stored() {
        let root = tempfile::tempdir().unwrap();
        let (printed, _) = run(
            select(0, &["1", "2"]),
            options(Mode::Compact, 10, root.path()),
            true,
        );
        assert_eq!(
            printed,
            "{\"results\":[{\"stmt\":0,\"cols\":[[\"value\",\"int\"]],\"rows\":[[\"1\"],[\"2\"]],\"count\":\"2\"}],\"success\":true}\n"
        );
        assert!(!root.path().join("results").exists());
    }
    #[test]
    fn a_large_result_is_stored_and_only_previewed() {
        let root = tempfile::tempdir().unwrap();
        let rows: Vec<String> = (0..25).map(|value| value.to_string()).collect();
        let borrowed: Vec<&str> = rows.iter().map(String::as_str).collect();
        let (printed, _) = run(
            select(0, &borrowed),
            options(Mode::Compact, 10, root.path()),
            true,
        );
        assert!(printed.contains("\"count\":\"25\""), "{printed}");
        assert!(printed.contains("\"file\":\""), "{printed}");
        assert_eq!(printed.matches("],[\"").count(), 9, "{printed}");
        let id = printed
            .split("\"id\":\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let stored =
            std::fs::read_to_string(root.path().join("results").join(id).join("0-0.jsonl"))
                .unwrap();
        assert_eq!(stored.lines().count(), 25);
        assert!(stored.starts_with("[\"0\"]\n"), "{stored}");
        let metadata =
            std::fs::read_to_string(root.path().join("results").join(id).join("metadata.json"))
                .unwrap();
        assert!(metadata.contains("\"origin\":\"cli\""), "{metadata}");
        assert!(metadata.contains("\"rows\":25"), "{metadata}");
    }
    #[test]
    fn full_mode_prints_every_row_and_stores_nothing() {
        let root = tempfile::tempdir().unwrap();
        let rows: Vec<String> = (0..25).map(|value| value.to_string()).collect();
        let borrowed: Vec<&str> = rows.iter().map(String::as_str).collect();
        let mut full = options(Mode::Compact, 10, root.path());
        full.full = true;
        let (printed, _) = run(select(0, &borrowed), full, true);
        assert!(printed.contains("\"count\":\"25\""), "{printed}");
        assert!(!printed.contains("\"file\""), "{printed}");
        assert!(!printed.contains("\"id\""), "{printed}");
        assert_eq!(printed.matches(r#""],["#).count(), 24, "{printed}");
        assert!(!root.path().join("results").exists());
    }
    #[test]
    fn full_mode_prints_a_value_past_the_byte_budget() {
        let root = tempfile::tempdir().unwrap();
        let big = "x".repeat(PREVIEW_BYTES + 1);
        let mut full = options(Mode::Compact, 10, root.path());
        full.full = true;
        let (printed, _) = run(select(0, &[big.as_str()]), full, true);
        assert!(
            printed.contains(&big),
            "the value is missing from the printed result"
        );
        assert!(!printed.contains("\"file\""), "{printed}");
    }
    #[test]
    fn a_preview_of_zero_stores_every_row() {
        let root = tempfile::tempdir().unwrap();
        let (printed, _) = run(
            select(0, &["1"]),
            options(Mode::Compact, 0, root.path()),
            true,
        );
        assert!(printed.contains("\"rows\":[]"), "{printed}");
        assert!(printed.contains("\"file\":\""), "{printed}");
    }
    #[test]
    fn a_row_past_the_byte_budget_is_stored_without_a_preview() {
        let root = tempfile::tempdir().unwrap();
        let big = "x".repeat(PREVIEW_BYTES + 1);
        let (printed, _) = run(
            select(0, &[big.as_str()]),
            options(Mode::Compact, 10, root.path()),
            true,
        );
        assert!(printed.contains("\"rows\":[]"), "{printed}");
        assert!(printed.contains("\"count\":\"1\""), "{printed}");
        assert!(printed.len() < 512, "{} bytes", printed.len());
    }
    #[test]
    fn an_error_and_skipped_statements_are_attached_to_their_statement() {
        let root = tempfile::tempdir().unwrap();
        let mut events = select(0, &["1"]);
        events.push(Event::StatementStart { index: 1 });
        events.push(Event::Error {
            index: Some(1),
            code: "mysql.1146.42S02".into(),
            message: "no such table".into(),
            outcome: "failed".into(),
        });
        events.push(Event::Skipped { index: 2 });
        let (printed, _) = run(events, options(Mode::Compact, 10, root.path()), false);
        assert_eq!(
            printed,
            "{\"results\":[{\"stmt\":0,\"cols\":[[\"value\",\"int\"]],\"rows\":[[\"1\"]],\"count\":\"1\"},{\"stmt\":1,\"error\":{\"code\":\"mysql.1146.42S02\",\"message\":\"no such table\",\"outcome\":\"failed\"}}],\"skipped\":[2],\"success\":false}\n"
        );
    }
    #[test]
    fn a_write_reports_affected_rows_and_an_empty_statement_its_index() {
        let root = tempfile::tempdir().unwrap();
        let events = vec![
            Event::Ready {
                protocol_version: sqlx_protocol::VERSION,
            },
            Event::Connected,
            Event::StatementStart { index: 0 },
            Event::Columns {
                index: 0,
                result: 0,
                columns: vec![],
            },
            Event::ResultEnd {
                index: 0,
                result: 0,
                rows: "0".into(),
                affected_rows: Some("3".into()),
            },
            Event::StatementEnd { index: 0 },
            Event::StatementStart { index: 1 },
            Event::StatementEnd { index: 1 },
        ];
        let (printed, _) = run(events, options(Mode::Compact, 10, root.path()), true);
        assert_eq!(
            printed,
            "{\"results\":[{\"stmt\":0,\"affected\":\"3\"},{\"stmt\":1}],\"success\":true}\n"
        );
    }
    #[test]
    fn encodings_are_kept_only_when_values_are_not_text() {
        let root = tempfile::tempdir().unwrap();
        let events = vec![
            Event::StatementStart { index: 0 },
            Event::Columns {
                index: 0,
                result: 0,
                columns: vec![
                    column("value", "int8"),
                    Column {
                        name: "blob".into(),
                        database_type: "bytea".into(),
                        encoding: "base64".into(),
                    },
                    Column {
                        name: "flag".into(),
                        database_type: "bool".into(),
                        encoding: "boolean".into(),
                    },
                ],
            },
            Event::ResultEnd {
                index: 0,
                result: 0,
                rows: "0".into(),
                affected_rows: None,
            },
            Event::StatementEnd { index: 0 },
        ];
        let (printed, _) = run(events, options(Mode::Compact, 10, root.path()), true);
        assert!(
            printed.contains("[\"value\",\"int8\"],[\"blob\",\"bytea\",\"base64\"],[\"flag\",\"bool\",\"boolean\"]"),
            "{printed}"
        );
    }
    #[test]
    fn a_failure_without_a_statement_closes_the_object() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = Writer::new(Vec::new(), options(Mode::Compact, 10, root.path()));
        writer
            .event(Event::Ready {
                protocol_version: sqlx_protocol::VERSION,
            })
            .unwrap();
        writer
            .failure("sqlx.error", "cannot start database worker")
            .unwrap();
        assert_eq!(
            String::from_utf8(writer.output).unwrap(),
            "{\"skipped\":[0,1,2],\"error\":{\"code\":\"sqlx.error\",\"message\":\"cannot start database worker\",\"outcome\":\"not_started\"},\"success\":false}\n"
        );
    }
    #[test]
    fn a_failure_mid_result_keeps_the_rows_and_reports_an_unknown_outcome() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = Writer::new(Vec::new(), options(Mode::Compact, 10, root.path()));
        writer.event(Event::StatementStart { index: 0 }).unwrap();
        writer
            .event(Event::Columns {
                index: 0,
                result: 0,
                columns: vec![column("value", "int8")],
            })
            .unwrap();
        writer
            .event(Event::Row {
                index: 0,
                result: 0,
                values: vec![Value::String("9".into())],
            })
            .unwrap();
        writer
            .failure("worker.incomplete", "worker stopped")
            .unwrap();
        assert_eq!(
            String::from_utf8(writer.output).unwrap(),
            "{\"results\":[{\"stmt\":0,\"cols\":[[\"value\",\"int8\"]],\"rows\":[[\"9\"]],\"error\":{\"code\":\"worker.incomplete\",\"message\":\"worker stopped\",\"outcome\":\"unknown\"}}],\"skipped\":[1,2],\"success\":false}\n"
        );
    }
    #[test]
    fn a_stored_result_that_fails_mid_way_points_at_its_partial_file() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = Writer::new(Vec::new(), options(Mode::Compact, 2, root.path()));
        writer.event(Event::StatementStart { index: 0 }).unwrap();
        writer
            .event(Event::Columns {
                index: 0,
                result: 0,
                columns: vec![column("value", "int8")],
            })
            .unwrap();
        for value in ["1", "2", "3"] {
            writer
                .event(Event::Row {
                    index: 0,
                    result: 0,
                    values: vec![Value::String(value.into())],
                })
                .unwrap();
        }
        writer
            .failure("worker.incomplete", "worker stopped")
            .unwrap();
        let printed = String::from_utf8(writer.output).unwrap();
        assert!(
            printed.contains("\"rows\":[[\"1\"],[\"2\"]],\"file\":\""),
            "{printed}"
        );
        assert!(printed.contains(",\"error\":{\"code\":\"worker.incomplete\",\"message\":\"worker stopped\",\"outcome\":\"unknown\"}}],\"skipped\":[1,2],\"id\":\""), "{printed}");
        assert!(printed.ends_with(",\"success\":false}\n"), "{printed}");
    }
    #[test]
    fn multiple_result_sets_of_one_statement_are_numbered() {
        let root = tempfile::tempdir().unwrap();
        let events = vec![
            Event::StatementStart { index: 0 },
            Event::Columns {
                index: 0,
                result: 0,
                columns: vec![column("value", "int8")],
            },
            Event::ResultEnd {
                index: 0,
                result: 0,
                rows: "0".into(),
                affected_rows: None,
            },
            Event::Columns {
                index: 0,
                result: 1,
                columns: vec![column("other", "int8")],
            },
            Event::ResultEnd {
                index: 0,
                result: 1,
                rows: "0".into(),
                affected_rows: None,
            },
            Event::StatementEnd { index: 0 },
        ];
        let (printed, _) = run(events, options(Mode::Compact, 10, root.path()), true);
        assert!(
            printed.contains(
                "{\"stmt\":0,\"cols\":[[\"value\",\"int8\"]],\"rows\":[],\"count\":\"0\"}"
            ),
            "{printed}"
        );
        assert!(printed.contains("{\"stmt\":0,\"set\":1,\"cols\":[[\"other\",\"int8\"]],\"rows\":[],\"count\":\"0\"}"), "{printed}");
    }
    #[test]
    fn the_events_mode_keeps_the_worker_envelope() {
        let root = tempfile::tempdir().unwrap();
        let (printed, _) = run(
            select(0, &["1"]),
            options(Mode::Events, 10, root.path()),
            true,
        );
        assert_eq!(
            printed,
            "{\"protocol_version\":1,\"datasource_id\":\"datasource\",\"events\":[{\"event\":\"ready\",\"protocol_version\":1},{\"event\":\"connected\"},{\"event\":\"statement_start\",\"index\":0},{\"event\":\"columns\",\"index\":0,\"result\":0,\"columns\":[{\"name\":\"value\",\"database_type\":\"MYSQL_TYPE_LONG\",\"encoding\":\"string\"}]},{\"event\":\"row\",\"index\":0,\"result\":0,\"values\":[\"1\"]},{\"event\":\"result_end\",\"index\":0,\"result\":0,\"rows\":\"1\",\"affected_rows\":null},{\"event\":\"statement_end\",\"index\":0}],\"success\":true}\n"
        );
        assert!(!root.path().join("results").exists());
    }
    #[test]
    fn a_batch_of_three_statements_keeps_one_item_each() {
        let root = tempfile::tempdir().unwrap();
        let mut events = select(0, &["1"]);
        events.extend(select(1, &["2", "3"]));
        events.extend(select(2, &[]));
        let (printed, _) = run(events, options(Mode::Compact, 10, root.path()), true);
        assert_eq!(
            printed,
            "{\"results\":[{\"stmt\":0,\"cols\":[[\"value\",\"int\"]],\"rows\":[[\"1\"]],\"count\":\"1\"},{\"stmt\":1,\"cols\":[[\"value\",\"int\"]],\"rows\":[[\"2\"],[\"3\"]],\"count\":\"2\"},{\"stmt\":2,\"cols\":[[\"value\",\"int\"]],\"rows\":[],\"count\":\"0\"}],\"success\":true}\n"
        );
    }
    #[test]
    fn mysql_types_are_reported_as_sql_types() {
        assert_eq!(short_type("MYSQL_TYPE_LONGLONG"), "bigint");
        assert_eq!(short_type("MYSQL_TYPE_VAR_STRING"), "varchar");
        assert_eq!(short_type("MYSQL_TYPE_NEWDECIMAL"), "decimal");
        assert_eq!(short_type("MYSQL_TYPE_TINY"), "tinyint");
        assert_eq!(short_type("MYSQL_TYPE_UNKNOWN_KIND"), "unknown_kind");
        assert_eq!(short_type("int4"), "int4");
        assert_eq!(short_type("Nullable(String)"), "nullable(string)");
    }
}
