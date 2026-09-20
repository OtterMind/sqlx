// Adapted from Chat2DB-Rust native_postgres.rs; standalone streaming worker. See NOTICE.
use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{NaiveDate, TimeDelta};
use futures_util::StreamExt;
use serde_json::Value;
use sqlx_protocol::{Action, Column, Database, Emitter, Event, Request, VERSION};
use std::{error::Error, io, time::Duration};
use tokio_postgres::{
    types::{FromSql, Kind, Type},
    Client, Config, NoTls,
};
use tokio_postgres_rustls::MakeRustlsConnect;

#[tokio::main]
async fn main() {
    let mut out = Emitter::new(io::stdout());
    let result: Result<bool> = async {
        out.send(Event::Ready {
            protocol_version: VERSION,
        })?;
        let req = Request::read()?;
        if req.connection.database_type != Database::Postgresql {
            bail!("wrong database worker");
        }
        match execute(&req, &mut out).await {
            Ok(()) => Ok(true),
            Err((index, error)) => {
                let db_error = error
                    .downcast_ref::<tokio_postgres::Error>()
                    .and_then(|e| e.as_db_error());
                let code = db_error.map_or_else(
                    || "postgres.execution_failed".into(),
                    |e| format!("postgres.{}", e.code().code()),
                );
                let outcome = if index.is_none() {
                    "not_started"
                } else if db_error.is_some() {
                    "failed"
                } else {
                    "unknown"
                };
                out.failed(
                    &req,
                    index,
                    &code,
                    &failure_message(&error, db_error),
                    outcome,
                )?;
                Ok(false)
            }
        }
    }
    .await;
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("PostgreSQL worker failed: {e}");
            std::process::exit(1);
        }
    }
}
/// `tokio_postgres` prints every database error as `db error`; keep the server's message instead.
fn failure_message(
    error: &anyhow::Error,
    db_error: Option<&tokio_postgres::error::DbError>,
) -> String {
    let Some(db_error) = db_error else {
        return error.to_string();
    };
    let mut message = db_error.message().to_owned();
    if let Some(detail) = db_error.detail() {
        message.push_str(&format!(" (detail: {detail})"));
    }
    if let Some(hint) = db_error.hint() {
        message.push_str(&format!(" (hint: {hint})"));
    }
    message
}
async fn execute(
    req: &Request,
    out: &mut Emitter<io::Stdout>,
) -> Result<(), (Option<usize>, anyhow::Error)> {
    let c = &req.connection;
    let mut cfg = Config::new();
    cfg.host(&c.host)
        .port(c.port)
        .user(&c.username)
        .password(&c.password)
        .connect_timeout(Duration::from_secs(15));
    if !c.database.is_empty() {
        cfg.dbname(&c.database);
    }
    for (key, value) in &c.properties {
        match key.as_str() {
            "application_name" => {
                cfg.application_name(value);
            }
            "options" => {
                cfg.options(value);
            }
            _ => {
                return Err((
                    None,
                    anyhow!("unsupported PostgreSQL connection property: {key}"),
                ))
            }
        }
    }
    let (client, task) = if c.tls == "disable" {
        cfg.ssl_mode(tokio_postgres::config::SslMode::Disable);
        let (client, connection) = cfg.connect(NoTls).await.map_err(|e| (None, e.into()))?;
        (client, tokio::spawn(connection))
    } else {
        let _ = rustls::crypto::ring::default_provider().install_default();
        cfg.ssl_mode(tokio_postgres::config::SslMode::Require);
        let (tls, _) = MakeRustlsConnect::with_native_certs()
            .map_err(|_| (None, anyhow!("TLS certificate roots unavailable")))?;
        let (client, connection) = cfg.connect(tls).await.map_err(|e| (None, e.into()))?;
        (client, tokio::spawn(connection))
    };
    let body = async {
        client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| (None, e.into()))?;
        out.send(Event::Connected).map_err(|e| (None, e))?;
        if req.action == Action::Execute {
            for (index, sql) in req.statements.iter().enumerate() {
                run_statement(&client, out, index, sql)
                    .await
                    .map_err(|e| (Some(index), e))?;
            }
        }
        Ok(())
    }
    .await;
    drop(client);
    let close = tokio::time::timeout(Duration::from_secs(5), task).await;
    body?;
    close
        .map_err(|_| (None, anyhow!("connection cleanup timed out")))?
        .map_err(|e| (None, e.into()))?
        .map_err(|e| (None, e.into()))?;
    out.send(Event::Complete { success: true })
        .map_err(|e| (None, e))
}
async fn run_statement(
    client: &Client,
    out: &mut Emitter<io::Stdout>,
    index: usize,
    sql: &str,
) -> Result<()> {
    out.send(Event::StatementStart { index })?;
    let stmt = client.prepare(sql).await?;
    let columns = stmt
        .columns()
        .iter()
        .map(|c| Column {
            name: c.name().into(),
            database_type: c.type_().name().into(),
            encoding: encoding(c.type_()).into(),
        })
        .collect();
    out.send(Event::Columns {
        index,
        result: 0,
        columns,
    })?;
    let stream = client
        .query_raw(
            &stmt,
            std::iter::empty::<&(dyn tokio_postgres::types::ToSql + Sync)>(),
        )
        .await?;
    tokio::pin!(stream);
    let mut rows = 0_u64;
    while let Some(row) = stream.next().await {
        let row = row?;
        let values = row
            .columns()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let raw: Option<Raw> = row.try_get(i)?;
                raw.map_or(Ok(Value::Null), |raw| decode(c.type_(), &raw.0))
            })
            .collect::<Result<Vec<_>>>()?;
        out.send(Event::Row {
            index,
            result: 0,
            values,
        })?;
        rows += 1;
    }
    let affected = stream.rows_affected().map(|v| v.to_string());
    out.send(Event::ResultEnd {
        index,
        result: 0,
        rows: rows.to_string(),
        affected_rows: affected,
    })?;
    out.send(Event::StatementEnd { index })
}
struct Raw(Vec<u8>);
impl<'a> FromSql<'a> for Raw {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Self(raw.to_vec()))
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}
fn base_type(t: &Type) -> &Type {
    if let Kind::Domain(inner) = t.kind() {
        base_type(inner)
    } else {
        t
    }
}
fn text_type(t: &Type) -> bool {
    matches!(
        t.name(),
        "text" | "varchar" | "bpchar" | "name" | "char" | "xml" | "json" | "jsonb" | "citext"
    ) || matches!(t.kind(), Kind::Enum(_))
}
fn encoding(t: &Type) -> &'static str {
    let t = base_type(t);
    if *t == Type::BOOL {
        "boolean"
    } else if text_type(t)
        || matches!(
            t.name(),
            "int2"
                | "int4"
                | "int8"
                | "oid"
                | "float4"
                | "float8"
                | "numeric"
                | "date"
                | "time"
                | "timestamp"
                | "timestamptz"
                | "uuid"
        )
    {
        "string"
    } else {
        "base64"
    }
}
fn number<const N: usize>(raw: &[u8]) -> Result<[u8; N]> {
    raw.try_into()
        .map_err(|_| anyhow!("invalid PostgreSQL binary value"))
}
fn decode(t: &Type, raw: &[u8]) -> Result<Value> {
    let t = base_type(t);
    let value = match t.name() {
        "bool" => return Ok(Value::Bool(raw == [1])),
        "int2" => i16::from_be_bytes(number(raw)?).to_string(),
        "int4" => i32::from_be_bytes(number(raw)?).to_string(),
        "int8" => i64::from_be_bytes(number(raw)?).to_string(),
        "oid" => u32::from_be_bytes(number(raw)?).to_string(),
        "float4" => f32::from_be_bytes(number(raw)?).to_string(),
        "float8" => f64::from_be_bytes(number(raw)?).to_string(),
        "numeric" => numeric(raw)?,
        "uuid" => uuid::Uuid::from_slice(raw)?.to_string(),
        "jsonb" => String::from_utf8(
            raw.strip_prefix(&[1])
                .ok_or_else(|| anyhow!("unknown JSONB encoding"))?
                .to_vec(),
        )?,
        "date" => {
            let days = i32::from_be_bytes(number(raw)?);
            match days {
                i32::MAX => "infinity".into(),
                i32::MIN => "-infinity".into(),
                _ => epoch()
                    .checked_add_signed(TimeDelta::days(i64::from(days)))
                    .ok_or_else(|| anyhow!("date out of range"))?
                    .to_string(),
            }
        }
        "time" => {
            let us = i64::from_be_bytes(number(raw)?);
            format!(
                "{:02}:{:02}:{:02}.{:06}",
                us / 3_600_000_000,
                (us / 60_000_000) % 60,
                (us / 1_000_000) % 60,
                us % 1_000_000
            )
        }
        "timestamp" | "timestamptz" => {
            let us = i64::from_be_bytes(number(raw)?);
            match us {
                i64::MAX => "infinity".into(),
                i64::MIN => "-infinity".into(),
                _ => {
                    let v = epoch()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .checked_add_signed(TimeDelta::microseconds(us))
                        .ok_or_else(|| anyhow!("timestamp out of range"))?;
                    format!(
                        "{}{}",
                        v.format("%Y-%m-%dT%H:%M:%S%.6f"),
                        if *t == Type::TIMESTAMPTZ { "Z" } else { "" }
                    )
                }
            }
        }
        _ if text_type(t) => String::from_utf8(raw.to_vec())?,
        _ => STANDARD.encode(raw),
    };
    Ok(Value::String(value))
}
fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()
}
// PostgreSQL NUMERIC wire groups are base 10000. Retain display scale exactly.
fn numeric(raw: &[u8]) -> Result<String> {
    if raw.len() < 8 || !raw.len().is_multiple_of(2) {
        bail!("invalid numeric value");
    }
    let count = u16::from_be_bytes(number(&raw[0..2])?) as usize;
    let weight = i16::from_be_bytes(number(&raw[2..4])?) as i32;
    let sign = u16::from_be_bytes(number(&raw[4..6])?);
    let scale = u16::from_be_bytes(number(&raw[6..8])?) as usize;
    if raw.len() != 8 + count * 2 {
        bail!("invalid numeric length");
    }
    match sign {
        0xC000 => return Ok("NaN".into()),
        0xD000 => return Ok("Infinity".into()),
        0xF000 => return Ok("-Infinity".into()),
        0 | 0x4000 => {}
        _ => bail!("invalid numeric sign"),
    }
    let digits = raw[8..]
        .chunks_exact(2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    if digits.iter().any(|d| *d >= 10000) {
        bail!("invalid numeric digit");
    }
    let digit = |position: i32| -> u16 {
        let index = weight - position;
        if index < 0 {
            0
        } else {
            digits.get(index as usize).copied().unwrap_or(0)
        }
    };
    let mut integer = String::new();
    for p in (0..=weight.max(0)).rev() {
        if integer.is_empty() {
            integer.push_str(&digit(p).to_string());
        } else {
            integer.push_str(&format!("{:04}", digit(p)));
        }
    }
    let mut fraction = String::new();
    for i in 1..=scale.div_ceil(4) {
        fraction.push_str(&format!("{:04}", digit(-(i as i32))));
    }
    fraction.truncate(scale);
    Ok(format!(
        "{}{}{}",
        if sign == 0x4000 { "-" } else { "" },
        integer,
        if scale > 0 {
            format!(".{fraction}")
        } else {
            String::new()
        }
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_scale_and_large_integer_are_exact() {
        let raw = [0, 2, 0, 0, 0, 0, 0, 4, 0, 123, 17, 148];
        assert_eq!(numeric(&raw).unwrap(), "123.4500");
        assert_eq!(
            decode(&Type::INT8, &9007199254740993_i64.to_be_bytes()).unwrap(),
            Value::String("9007199254740993".into())
        );
    }
}
