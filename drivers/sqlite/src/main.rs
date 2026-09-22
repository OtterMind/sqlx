//! SQLite worker: opens a local database file and runs one statement per request entry.
//!
//! SQLite is dynamically typed, so a column reports the type it was declared with when the schema
//! names one and otherwise the kind of the first value it carries. Values keep the protocol
//! encoding: integers stay exact text, floats use the shortest round-trip form, and blobs or text
//! that is not valid UTF-8 use Base64.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{types::ValueRef, Connection, OpenFlags, Statement};
use serde_json::Value as Json;
use sqlx_protocol::{Action, Column, Database, Emitter, Event, Request, VERSION};
use std::{
    io::{self, Write},
    time::Duration,
};

/// Connection options a caller may set with `--property`.
const OPTIONS: [&str; 2] = ["mode", "busy_timeout"];
const BUSY_TIMEOUT_MS: u64 = 5_000;

fn main() {
    let mut out = Emitter::new(io::stdout());
    let result = handshake(&mut out);
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("SQLite worker failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handshake<W: Write>(out: &mut Emitter<W>) -> Result<bool> {
    out.send(Event::Ready {
        protocol_version: VERSION,
    })?;
    let req = Request::read()?;
    if req.connection.database_type != Database::Sqlite {
        bail!("wrong database worker");
    }
    let connection = match open(&req) {
        Ok(connection) => connection,
        Err(error) => {
            out.failed(
                &req,
                None,
                "sqlite.open_failed",
                &error.to_string(),
                "not_started",
            )?;
            return Ok(false);
        }
    };
    out.send(Event::Connected)?;
    if req.action == Action::Execute {
        if let Err((index, error)) = execute(&connection, out, &req.statements) {
            let (code, outcome) = classify(&error);
            out.failed(&req, Some(index), &code, &format!("{error:#}"), outcome)?;
            return Ok(false);
        }
    }
    out.send(Event::Complete { success: true })?;
    Ok(true)
}

fn open(req: &Request) -> Result<Connection> {
    let c = &req.connection;
    for key in c.properties.keys() {
        if !OPTIONS.contains(&key.as_str()) {
            bail!(
                "unknown SQLite option {key}; supported options are {}",
                OPTIONS.join(", ")
            );
        }
    }
    let path = c.database.trim();
    if path.is_empty() {
        bail!("SQLite needs the database file in --database or --path");
    }
    let flags = match c.properties.get("mode").map(String::as_str) {
        Some("ro") | Some("read-only") => OpenFlags::SQLITE_OPEN_READ_ONLY,
        Some("rw") | Some("read-write") | None => {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        }
        Some(other) => bail!("SQLite mode expects ro or rw, not {other}"),
    };
    let connection = if path == ":memory:" {
        Connection::open_in_memory()?
    } else {
        Connection::open_with_flags(path, flags)
            .with_context(|| format!("cannot open the SQLite database {path}"))?
    };
    let timeout = match c.properties.get("busy_timeout") {
        Some(value) => value.parse().context("busy_timeout expects milliseconds")?,
        None => BUSY_TIMEOUT_MS,
    };
    connection.busy_timeout(Duration::from_millis(timeout))?;
    Ok(connection)
}

fn execute<W: Write>(
    connection: &Connection,
    out: &mut Emitter<W>,
    statements: &[String],
) -> std::result::Result<(), (usize, anyhow::Error)> {
    for (index, sql) in statements.iter().enumerate() {
        statement(connection, out, index, sql).map_err(|error| (index, error))?;
    }
    Ok(())
}

fn statement<W: Write>(
    connection: &Connection,
    out: &mut Emitter<W>,
    index: usize,
    sql: &str,
) -> Result<()> {
    out.send(Event::StatementStart { index })?;
    let mut prepared = connection
        .prepare(sql)
        .with_context(|| format!("cannot prepare statement {index}"))?;
    if prepared.column_count() == 0 {
        let affected = prepared.execute([])?;
        out.send(Event::Columns {
            index,
            result: 0,
            columns: vec![],
        })?;
        out.send(Event::ResultEnd {
            index,
            result: 0,
            rows: "0".into(),
            affected_rows: Some(affected.to_string()),
        })?;
        out.send(Event::StatementEnd { index })?;
        return Ok(());
    }
    let declared = declared_types(&prepared);
    let mut rows = prepared.query([])?;
    // The first row decides the encoding of every column, because SQLite stores types per value.
    let first = rows.next()?;
    let columns: Vec<Column> = declared
        .iter()
        .enumerate()
        .map(|(position, (name, declared))| Column {
            name: name.clone(),
            database_type: declared.clone().unwrap_or_else(|| {
                kind(first.as_ref().and_then(|row| row.get_ref(position).ok())).to_owned()
            }),
            encoding: encoding(first.as_ref().and_then(|row| row.get_ref(position).ok()))
                .to_owned(),
        })
        .collect();
    out.send(Event::Columns {
        index,
        result: 0,
        columns,
    })?;
    let width = declared.len();
    let mut count = 0_u64;
    if let Some(row) = first {
        send_row(out, index, row, width)?;
        count += 1;
    }
    while let Some(row) = rows.next()? {
        send_row(out, index, row, width)?;
        count += 1;
    }
    out.send(Event::ResultEnd {
        index,
        result: 0,
        rows: count.to_string(),
        affected_rows: None,
    })?;
    out.send(Event::StatementEnd { index })?;
    Ok(())
}

fn send_row<W: Write>(
    out: &mut Emitter<W>,
    index: usize,
    row: &rusqlite::Row<'_>,
    width: usize,
) -> Result<()> {
    let values = (0..width)
        .map(|position| encode(row.get_ref(position)?))
        .collect::<Result<Vec<Json>>>()?;
    out.send(Event::Row {
        index,
        result: 0,
        values,
    })?;
    Ok(())
}

/// Column names with the type the schema declares, which is missing for expressions.
fn declared_types(statement: &Statement<'_>) -> Vec<(String, Option<String>)> {
    statement
        .columns()
        .iter()
        .map(|column| {
            (
                column.name().to_owned(),
                column.decl_type().map(|kind| {
                    kind.split('(')
                        .next()
                        .unwrap_or(kind)
                        .trim()
                        .to_ascii_lowercase()
                }),
            )
        })
        .collect()
}

fn encode(value: ValueRef<'_>) -> Result<Json> {
    Ok(match value {
        ValueRef::Null => Json::Null,
        ValueRef::Integer(number) => Json::String(number.to_string()),
        ValueRef::Real(number) => Json::String(number.to_string()),
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => Json::String(text.into()),
            Err(_) => Json::String(STANDARD.encode(bytes)),
        },
        ValueRef::Blob(bytes) => Json::String(STANDARD.encode(bytes)),
    })
}

fn kind(value: Option<ValueRef<'_>>) -> &'static str {
    match value {
        Some(ValueRef::Integer(_)) => "integer",
        Some(ValueRef::Real(_)) => "real",
        Some(ValueRef::Text(_)) => "text",
        Some(ValueRef::Blob(_)) => "blob",
        _ => "dynamic",
    }
}

fn encoding(value: Option<ValueRef<'_>>) -> &'static str {
    match value {
        Some(ValueRef::Blob(_)) => "base64",
        Some(ValueRef::Text(bytes)) if std::str::from_utf8(bytes).is_err() => "base64",
        _ => "string",
    }
}

fn classify(error: &anyhow::Error) -> (String, &'static str) {
    match error.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::SqliteFailure(failure, _)) => {
            (format!("sqlite.{}", failure.extended_code), "failed")
        }
        _ => ("sqlite.execution_failed".into(), "unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(statements: &[&str]) -> (Vec<Json>, Option<String>) {
        let connection = Connection::open_in_memory().unwrap();
        let mut buffer = Vec::new();
        let statements: Vec<String> = statements.iter().map(|sql| (*sql).to_owned()).collect();
        let failure = {
            let mut out = Emitter::new(&mut buffer);
            execute(&connection, &mut out, &statements)
                .err()
                .map(|(_, error)| format!("{error:#}"))
        };
        let text = String::from_utf8(buffer).unwrap();
        (
            text.lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect(),
            failure,
        )
    }
    fn rows(events: &[Json]) -> Vec<Json> {
        events
            .iter()
            .filter(|event| event["event"] == "row")
            .map(|event| event["values"].clone())
            .collect()
    }
    fn table_columns(events: &[Json]) -> Json {
        events
            .iter()
            .find(|event| {
                event["event"] == "columns"
                    && event["columns"].as_array().is_some_and(|c| !c.is_empty())
            })
            .expect("a result set")["columns"]
            .clone()
    }
    #[test]
    fn statements_report_columns_rows_and_affected_counts() {
        let (events, failure) = run(&[
            "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL, payload BLOB)",
            "INSERT INTO t VALUES (1, 'ada', 9.5, X'00FF'), (2, 'bob', 8.25, NULL)",
            "SELECT id, name, score, payload FROM t ORDER BY id",
        ]);
        assert!(failure.is_none(), "{failure:?}");
        let columns = table_columns(&events);
        assert_eq!(columns[0]["name"], "id");
        assert_eq!(columns[0]["database_type"], "integer");
        assert_eq!(columns[1]["database_type"], "text");
        assert_eq!(columns[1]["encoding"], "string");
        assert_eq!(columns[3]["encoding"], "base64");
        assert_eq!(
            rows(&events),
            vec![
                Json::Array(vec!["1".into(), "ada".into(), "9.5".into(), "AP8=".into()]),
                Json::Array(vec!["2".into(), "bob".into(), "8.25".into(), Json::Null]),
            ]
        );
        let affected: Vec<Json> = events
            .iter()
            .filter(|event| event["event"] == "result_end")
            .filter_map(|event| event["affected_rows"].as_str().map(Into::into))
            .collect();
        assert_eq!(affected, vec![Json::from("0"), Json::from("2")]);
    }
    #[test]
    fn expressions_report_the_kind_of_their_values() {
        let (events, failure) = run(&["SELECT 1 + 1 AS value, 'x' AS label, NULL AS empty"]);
        assert!(failure.is_none(), "{failure:?}");
        let columns = table_columns(&events);
        assert_eq!(columns[0]["database_type"], "integer");
        assert_eq!(columns[1]["database_type"], "text");
        assert_eq!(columns[2]["database_type"], "dynamic");
        assert_eq!(
            rows(&events),
            vec![Json::Array(vec!["2".into(), "x".into(), Json::Null])]
        );
    }
    #[test]
    fn a_failing_statement_reports_its_sqlite_code() {
        let connection = Connection::open_in_memory().unwrap();
        let mut out = Emitter::new(Vec::new());
        let error = execute(&connection, &mut out, &["SELECT * FROM missing".into()]).unwrap_err();
        assert_eq!(error.0, 0);
        let (code, outcome) = classify(&error.1);
        assert!(code.starts_with("sqlite."), "{code}");
        assert_eq!(outcome, "failed");
    }
    #[test]
    fn connection_options_are_validated() {
        let connection = |properties: &[(&str, &str)]| sqlx_protocol::Connection {
            database_type: Database::Sqlite,
            host: String::new(),
            port: 0,
            database: ":memory:".into(),
            service: String::new(),
            username: String::new(),
            password: String::new(),
            tls: "disable".into(),
            properties: properties
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        };
        let request = |properties: &[(&str, &str)]| Request {
            protocol_version: VERSION,
            action: Action::Execute,
            connection: connection(properties),
            statements: vec![],
            driver_jars: vec![],
            driver_class: String::new(),
        };
        assert!(open(&request(&[("mode", "rw"), ("busy_timeout", "1000")])).is_ok());
        assert!(open(&request(&[("mode", "shared")])).is_err());
        assert!(open(&request(&[("cache", "shared")])).is_err());
    }
}
