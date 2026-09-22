//! DuckDB worker: opens a local database file and runs one statement per request entry.
//!
//! DuckDB keeps exact numbers out of JSON's range, so integers, huge integers and decimals stay
//! text; nested values (list, struct, map, array, union) become canonical JSON text, and blobs use
//! Base64. Temporal values are rendered as ISO text so a caller does not have to know the unit.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use duckdb::{
    types::{TimeUnit, Type, Value},
    AccessMode, Config, Connection, Statement,
};
use serde_json::{Map, Value as Json};
use sqlx_protocol::{Action, Column, Database, Emitter, Event, Request, VERSION};
use std::io::{self, Write};

/// Connection options a caller may set with `--property`.
const OPTIONS: [&str; 2] = ["read_only", "threads"];

fn main() {
    let mut out = Emitter::new(io::stdout());
    let result = handshake(&mut out);
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("DuckDB worker failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handshake<W: Write>(out: &mut Emitter<W>) -> Result<bool> {
    out.send(Event::Ready {
        protocol_version: VERSION,
    })?;
    let req = Request::read()?;
    if req.connection.database_type != Database::Duckdb {
        bail!("wrong database worker");
    }
    let connection = match open(&req) {
        Ok(connection) => connection,
        Err(error) => {
            out.failed(
                &req,
                None,
                "duckdb.open_failed",
                &format!("{error:#}"),
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
                "unknown DuckDB option {key}; supported options are {}",
                OPTIONS.join(", ")
            );
        }
    }
    let path = c.database.trim();
    if path.is_empty() {
        bail!("DuckDB needs the database file in --database or --path");
    }
    let mut config = Config::default();
    if c.properties
        .get("read_only")
        .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "yes"))
    {
        config = config.access_mode(AccessMode::ReadOnly)?;
    }
    if let Some(threads) = c.properties.get("threads") {
        config = config.threads(
            threads
                .parse()
                .context("DuckDB threads expects a whole number")?,
        )?;
    }
    let connection = if path == ":memory:" {
        Connection::open_in_memory_with_flags(config)
    } else {
        Connection::open_with_flags(path, config)
    }
    .with_context(|| format!("cannot open the DuckDB database {path}"))?;
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
    single_statement(sql)?;
    let mut prepared = connection
        .prepare(sql)
        .with_context(|| format!("cannot prepare statement {index}"))?;
    // DuckDB exposes result metadata only after the statement ran, so execute first and then read
    // the result of that same execution.
    let affected = prepared.execute([])?;
    if prepared.column_count() == 0 {
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
    out.send(Event::Columns {
        index,
        result: 0,
        columns: column_metadata(&prepared),
    })?;
    let mut rows = prepared.raw_query();
    let mut count = 0_u64;
    while let Some(row) = rows.next()? {
        let values = (0..row.as_ref().column_count())
            .map(|position| {
                let value: Value = row
                    .get(position)
                    .with_context(|| format!("cannot read column {position}"))?;
                Ok(encode(&value))
            })
            .collect::<Result<Vec<Json>>>()?;
        out.send(Event::Row {
            index,
            result: 0,
            values,
        })?;
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

/// DuckDB runs every statement in one string but reports only the first result set, so reject a
/// command that carries more than one statement instead of silently dropping the rest.
fn single_statement(sql: &str) -> Result<()> {
    let bytes = sql.as_bytes();
    let mut position = 0;
    let mut terminated = 0;
    let mut has_text = false;
    while position < bytes.len() {
        match bytes[position] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[position];
                position += 1;
                while position < bytes.len() {
                    if bytes[position] == quote {
                        if bytes.get(position + 1) == Some(&quote) {
                            position += 2;
                            continue;
                        }
                        break;
                    }
                    position += 1;
                }
                has_text = true;
            }
            b'$' if bytes.get(position + 1) == Some(&b'$') => {
                position += 2;
                while position + 1 < bytes.len()
                    && !(bytes[position] == b'$' && bytes[position + 1] == b'$')
                {
                    position += 1;
                }
                position += 2;
                has_text = true;
            }
            b'-' if bytes.get(position + 1) == Some(&b'-') => {
                while position < bytes.len() && bytes[position] != b'\n' {
                    position += 1;
                }
            }
            b'/' if bytes.get(position + 1) == Some(&b'*') => {
                position += 2;
                while position + 1 < bytes.len()
                    && !(bytes[position] == b'*' && bytes[position + 1] == b'/')
                {
                    position += 1;
                }
                position += 2;
            }
            b';' if has_text => {
                terminated += 1;
                has_text = false;
            }
            // An empty statement (`;;`) ends nothing and must not count as text.
            b';' => {}
            byte if !byte.is_ascii_whitespace() => has_text = true,
            _ => {}
        }
        position += 1;
    }
    if terminated > 0 && has_text {
        return Err(MultipleStatements.into());
    }
    Ok(())
}

/// A command that carried more than one statement, which is rejected before anything runs.
#[derive(Debug)]
struct MultipleStatements;
impl std::fmt::Display for MultipleStatements {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "one statement per --command; DuckDB would run the rest without reporting it",
        )
    }
}
impl std::error::Error for MultipleStatements {}

fn column_metadata(statement: &Statement<'_>) -> Vec<Column> {
    (0..statement.column_count())
        .map(|position| {
            let kind = Type::from(&statement.column_type(position));
            Column {
                name: statement
                    .column_name(position)
                    .cloned()
                    .unwrap_or_else(|_| format!("column{position}")),
                database_type: type_name(&kind),
                encoding: type_encoding(&kind).to_owned(),
            }
        })
        .collect()
}

/// DuckDB type names, with the names a SQL caller expects for the common ones.
fn type_name(kind: &Type) -> String {
    match kind {
        Type::Text => "varchar".into(),
        Type::Date32 => "date".into(),
        Type::Time64 => "time".into(),
        other => other.to_string().to_ascii_lowercase(),
    }
}

fn type_encoding(kind: &Type) -> &'static str {
    match kind {
        Type::Blob | Type::Geometry => "base64",
        Type::List(_) | Type::Struct(_) | Type::Map(..) | Type::Array(..) | Type::Union => "json",
        _ => "string",
    }
}

fn encode(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Boolean(value) => Json::String(value.to_string()),
        Value::TinyInt(number) => Json::String(number.to_string()),
        Value::SmallInt(number) => Json::String(number.to_string()),
        Value::Int(number) => Json::String(number.to_string()),
        Value::BigInt(number) => Json::String(number.to_string()),
        Value::HugeInt(number) => Json::String(number.to_string()),
        Value::UHugeInt(number) => Json::String(number.to_string()),
        Value::UTinyInt(number) => Json::String(number.to_string()),
        Value::USmallInt(number) => Json::String(number.to_string()),
        Value::UInt(number) => Json::String(number.to_string()),
        Value::UBigInt(number) => Json::String(number.to_string()),
        Value::Float(number) => Json::String(number.to_string()),
        Value::Double(number) => Json::String(number.to_string()),
        Value::Decimal(number) => Json::String(number.to_string()),
        Value::Timestamp(unit, value) => Json::String(timestamp(*unit, *value)),
        Value::Date32(days) => Json::String(date(*days)),
        Value::Time64(unit, value) => Json::String(time(*unit, *value)),
        Value::Interval {
            months,
            days,
            nanos,
        } => Json::String(interval(*months, *days, *nanos)),
        Value::Text(text) => Json::String(text.clone()),
        Value::Enum(label) => Json::String(label.clone()),
        Value::Blob(bytes) | Value::Geometry(bytes) => Json::String(STANDARD.encode(bytes)),
        Value::List(items) | Value::Array(items) => {
            Json::String(Json::Array(items.iter().map(encode).collect()).to_string())
        }
        Value::Struct(fields) => {
            let mut object = Map::new();
            for (name, value) in fields.iter() {
                object.insert(name.clone(), encode(value));
            }
            Json::String(Json::Object(object).to_string())
        }
        Value::Map(entries) => {
            let mut object = Map::new();
            for (key, value) in entries.iter() {
                object.insert(json_key(key), encode(value));
            }
            Json::String(Json::Object(object).to_string())
        }
        Value::Union(value) => encode(value),
        // DuckDB may add value kinds; keep them readable instead of failing the result.
        other => Json::String(format!("{other:?}")),
    }
}

/// Map keys must be text; anything else keeps its rendered form.
fn json_key(value: &Value) -> String {
    match encode(value) {
        Json::String(text) => text,
        other => other.to_string(),
    }
}

fn timestamp(unit: TimeUnit, value: i64) -> String {
    let (seconds, fraction) = match unit {
        TimeUnit::Second => (value, 0),
        TimeUnit::Millisecond => (value.div_euclid(1_000), value.rem_euclid(1_000) * 1_000_000),
        TimeUnit::Microsecond => (
            value.div_euclid(1_000_000),
            value.rem_euclid(1_000_000) * 1_000,
        ),
        TimeUnit::Nanosecond => (
            value.div_euclid(1_000_000_000),
            value.rem_euclid(1_000_000_000),
        ),
    };
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    format!(
        "{}T{}",
        date(days as i32),
        clock(time as u64, fraction as u64)
    )
}

fn date(days: i32) -> String {
    // Civil date from the days since 1970-01-01 (Howard Hinnant's algorithm).
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year_of_era + era * 400 + i32::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn time(unit: TimeUnit, value: i64) -> String {
    let fraction = match unit {
        TimeUnit::Second => 0,
        TimeUnit::Millisecond => value.rem_euclid(1_000) * 1_000_000,
        TimeUnit::Microsecond => value.rem_euclid(1_000_000) * 1_000,
        TimeUnit::Nanosecond => value.rem_euclid(1_000_000_000),
    };
    let seconds = match unit {
        TimeUnit::Second => value,
        TimeUnit::Millisecond => value.div_euclid(1_000),
        TimeUnit::Microsecond => value.div_euclid(1_000_000),
        TimeUnit::Nanosecond => value.div_euclid(1_000_000_000),
    };
    clock(seconds.rem_euclid(86_400) as u64, fraction as u64)
}

fn clock(seconds: u64, nanos: u64) -> String {
    let base = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    );
    if nanos == 0 {
        return base;
    }
    let fraction = format!("{nanos:09}");
    format!("{base}.{}", fraction.trim_end_matches('0'))
}

fn interval(months: i32, days: i32, nanos: i64) -> String {
    let seconds = nanos.div_euclid(1_000_000_000);
    let fraction = nanos.rem_euclid(1_000_000_000);
    let mut parts = Vec::new();
    if months != 0 {
        parts.push(format!("{months} months"));
    }
    if days != 0 {
        parts.push(format!("{days} days"));
    }
    if seconds != 0 || fraction != 0 || parts.is_empty() {
        parts.push(format!("{seconds}.{fraction:09}s"));
    }
    parts.join(" ")
}

fn classify(error: &anyhow::Error) -> (String, &'static str) {
    if error.downcast_ref::<MultipleStatements>().is_some() {
        return ("duckdb.multiple_statements".into(), "failed");
    }
    match error.downcast_ref::<duckdb::Error>() {
        Some(duckdb::Error::DuckDBFailure(_, Some(message))) => {
            (duckdb_error_code(message), "failed")
        }
        Some(duckdb::Error::DuckDBFailure(..)) => ("duckdb.failure".into(), "failed"),
        _ => ("duckdb.execution_failed".into(), "unknown"),
    }
}

/// DuckDB reports failures as one message; keep its type prefix when it names one.
fn duckdb_error_code(message: &str) -> String {
    let prefix: String = message
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if prefix.is_empty() {
        "duckdb.failure".into()
    } else {
        format!("duckdb.{}", prefix.to_ascii_lowercase())
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
    fn columns(events: &[Json], index: usize) -> Json {
        events
            .iter()
            .find(|event| event["event"] == "columns" && event["index"] == index)
            .expect("columns of that statement")["columns"]
            .clone()
    }
    fn rows(events: &[Json], index: usize) -> Vec<Json> {
        events
            .iter()
            .filter(|event| event["event"] == "row" && event["index"] == index)
            .map(|event| event["values"].clone())
            .collect()
    }
    #[test]
    fn exact_numbers_dates_and_blobs_keep_their_shape() {
        let (events, failure) = run(&[
            "CREATE TABLE t (id BIGINT, score DECIMAL(18,3), amount HUGEINT, when_ TIMESTAMP, day DATE, payload BLOB)",
            "INSERT INTO t VALUES (9007199254740993, 123.4500, 170141183460469231731687303715884105727, TIMESTAMP '2024-02-29 13:45:06.500', DATE '2024-02-29', '\\x00\\xFF'::BLOB)",
            "SELECT id, score, amount, when_, day, payload FROM t",
        ]);
        assert!(failure.is_none(), "{failure:?}");
        let columns = columns(&events, 2);
        assert_eq!(columns[0]["database_type"], "bigint");
        assert_eq!(columns[1]["database_type"], "decimal");
        assert_eq!(columns[2]["database_type"], "decimal");
        assert_eq!(columns[3]["database_type"], "timestamp");
        assert_eq!(columns[4]["database_type"], "date");
        assert_eq!(columns[5]["encoding"], "base64");
        assert_eq!(
            rows(&events, 2)[0],
            Json::Array(vec![
                "9007199254740993".into(),
                "123.450".into(),
                "170141183460469231731687303715884105727".into(),
                "2024-02-29T13:45:06.5".into(),
                "2024-02-29".into(),
                "AP8=".into(),
            ])
        );
    }
    #[test]
    fn writes_report_their_row_count_as_a_count_result_set() {
        // DuckDB answers every statement with a result set; a write reports its count in `Count`.
        let (events, failure) = run(&[
            "CREATE TABLE t (id INTEGER)",
            "INSERT INTO t VALUES (1), (2), (3)",
            "UPDATE t SET id = id + 1",
            "DELETE FROM t WHERE id > 2",
        ]);
        assert!(failure.is_none(), "{failure:?}");
        assert_eq!(columns(&events, 1)[0]["name"], "Count");
        assert_eq!(rows(&events, 0), Vec::<Json>::new());
        assert_eq!(rows(&events, 1), vec![Json::Array(vec!["3".into()])]);
        assert_eq!(rows(&events, 2), vec![Json::Array(vec!["3".into()])]);
        assert_eq!(rows(&events, 3), vec![Json::Array(vec!["2".into()])]);
    }
    #[test]
    fn nested_values_become_json_text() {
        let (events, failure) =
            run(&["SELECT [1, 2] AS items, {'a': 1} AS record, MAP(['k'], [2]) AS pairs"]);
        assert!(failure.is_none(), "{failure:?}");
        let columns = columns(&events, 0);
        assert_eq!(columns[0]["database_type"], "list");
        assert_eq!(columns[0]["encoding"], "json");
        assert_eq!(columns[1]["database_type"], "struct");
        assert_eq!(columns[2]["database_type"], "map");
        // Nested numbers keep the same text encoding as scalar cells, so nothing loses precision.
        let row = rows(&events, 0)[0].clone();
        assert_eq!(row[0], "[\"1\",\"2\"]");
        assert_eq!(row[1], "{\"a\":\"1\"}");
        assert_eq!(row[2], "{\"k\":\"2\"}");
    }
    #[test]
    fn a_failing_statement_names_the_duckdb_error() {
        let connection = Connection::open_in_memory().unwrap();
        let mut out = Emitter::new(Vec::new());
        let error = execute(&connection, &mut out, &["SELECT * FROM missing".into()]).unwrap_err();
        let (code, outcome) = classify(&error.1);
        assert!(code.starts_with("duckdb."), "{code}");
        assert_eq!(outcome, "failed");
    }
    #[test]
    fn only_one_statement_per_command_is_accepted() {
        assert!(single_statement("SELECT 1").is_ok());
        assert!(single_statement("SELECT 1;").is_ok());
        assert!(single_statement("SELECT 1;   ").is_ok());
        assert!(single_statement("SELECT 1;;").is_ok());
        assert!(single_statement("SELECT ';' AS semi").is_ok());
        assert!(single_statement("SELECT $$a;b$$ AS dollar").is_ok());
        assert!(single_statement("-- note; one statement\nSELECT 1").is_ok());
        assert!(single_statement("/* one; */ SELECT 1").is_ok());
        assert!(single_statement("SELECT 'it''s; fine'").is_ok());
        assert!(single_statement("SELECT 1; SELECT 2").is_err());
        assert!(single_statement("INSERT INTO t VALUES (1); INSERT INTO t VALUES (2)").is_err());
        assert!(single_statement("SELECT 1; -- note\nSELECT 2").is_err());
        let rejection = single_statement("SELECT 1; SELECT 2").unwrap_err();
        assert_eq!(classify(&rejection).0, "duckdb.multiple_statements");
        assert_eq!(classify(&rejection).1, "failed");
    }
    #[test]
    fn temporal_helpers_render_iso_text() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(-1), "1969-12-31");
        assert_eq!(
            timestamp(TimeUnit::Microsecond, 1_700_000_000_000_000),
            "2023-11-14T22:13:20"
        );
        assert_eq!(time(TimeUnit::Microsecond, 3_723_000_000), "01:02:03");
        assert_eq!(time(TimeUnit::Microsecond, 3_723_500_000), "01:02:03.5");
    }
}
