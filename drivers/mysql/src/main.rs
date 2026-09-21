// Connection/result handling adapted from Chat2DB-Rust native_mysql.rs. See NOTICE.
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use mysql_async::{prelude::Queryable, Column as MysqlColumn, Conn, OptsBuilder, SslOpts, Value};
use serde_json::Value as Json;
use sqlx_protocol::{Action, Column, Database, Emitter, Event, Request, VERSION};
use std::{io, time::Duration};

#[tokio::main]
async fn main() {
    let mut out = Emitter::new(io::stdout());
    let result = async {
        out.send(Event::Ready {
            protocol_version: VERSION,
        })?;
        let req = Request::read()?;
        if !matches!(
            req.connection.database_type,
            Database::Mysql
                | Database::Mariadb
                | Database::Tidb
                | Database::Starrocks
                | Database::Doris
        ) {
            bail!("wrong database worker");
        }
        let result = execute(&req, &mut out).await;
        match result {
            Ok(()) => Ok(true),
            Err((index, error)) => {
                let (code, outcome) = match error.downcast_ref::<mysql_async::Error>() {
                    Some(mysql_async::Error::Server(e)) => {
                        (format!("mysql.{}.{}", e.code, e.state), "failed")
                    }
                    _ => (
                        "mysql.execution_failed".into(),
                        if index.is_some() {
                            "unknown"
                        } else {
                            "not_started"
                        },
                    ),
                };
                out.failed(&req, index, &code, &error.to_string(), outcome)?;
                Ok(false)
            }
        }
    }
    .await;
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("MySQL worker failed: {error}");
            std::process::exit(1);
        }
    }
}

async fn execute(
    req: &Request,
    out: &mut Emitter<io::Stdout>,
) -> Result<(), (Option<usize>, anyhow::Error)> {
    let c = &req.connection;
    if !c.properties.is_empty() {
        return Err((
            None,
            anyhow::anyhow!("native MySQL currently accepts dedicated connection options only"),
        ));
    }
    let opts = OptsBuilder::default()
        .ip_or_hostname(c.host.clone())
        .tcp_port(c.port)
        .prefer_socket(false)
        .user(Some(c.username.clone()))
        .pass(Some(c.password.clone()))
        .db_name((!c.database.is_empty()).then(|| c.database.clone()))
        .ssl_opts((c.tls != "disable").then(SslOpts::default))
        .init(vec!["SET NAMES utf8mb4".to_string()]);
    let mut conn = tokio::time::timeout(Duration::from_secs(15), Conn::new(opts))
        .await
        .map_err(|_| {
            (
                None,
                anyhow::anyhow!(
                    "Connection to MySQL at {}:{} timed out after 15 seconds.",
                    c.host,
                    c.port
                ),
            )
        })?
        .map_err(|e| (None, connection_error(&c.host, c.port, e)))?;
    let body = async {
        conn.ping().await.map_err(|e| (None, e.into()))?;
        out.send(Event::Connected).map_err(|e| (None, e))?;
        if req.action == Action::Execute {
            for (index, sql) in req.statements.iter().enumerate() {
                run_statement(&mut conn, out, index, sql)
                    .await
                    .map_err(|e| (Some(index), e))?;
            }
        }
        Ok(())
    }
    .await;
    // Closing a connection must not commit any explicit transaction left by the caller.
    let close = conn.disconnect().await;
    body?;
    close.map_err(|e| (None, e.into()))?;
    out.send(Event::Complete { success: true })
        .map_err(|e| (None, e))
}

fn connection_error(host: &str, port: u16, error: mysql_async::Error) -> anyhow::Error {
    let error = anyhow::Error::from(error);
    if error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|cause| cause.kind() == io::ErrorKind::ConnectionRefused)
    {
        anyhow::anyhow!(
            "MySQL at {host}:{port} refused the connection. Check that MySQL is running and the address is correct."
        )
    } else {
        error
    }
}

async fn run_statement(
    conn: &mut Conn,
    out: &mut Emitter<io::Stdout>,
    index: usize,
    sql: &str,
) -> Result<()> {
    out.send(Event::StatementStart { index })?;
    let mut query = conn.query_iter(sql).await?;
    let mut result = 0;
    while let Some(columns) = query.columns() {
        let metadata: Vec<Column> = columns
            .iter()
            .map(|c| Column {
                name: c.name_str().into_owned(),
                database_type: format!("{:?}", c.column_type()),
                encoding: if binary(c) { "base64" } else { "string" }.into(),
            })
            .collect();
        let affected = columns
            .is_empty()
            .then(|| query.affected_rows().to_string());
        out.send(Event::Columns {
            index,
            result,
            columns: metadata,
        })?;
        let mut rows = 0_u64;
        while let Some(row) = query.next().await? {
            let values = row
                .unwrap()
                .into_iter()
                .zip(columns.iter())
                .map(|(v, c)| encode(v, c))
                .collect::<Result<Vec<_>>>()?;
            out.send(Event::Row {
                index,
                result,
                values,
            })?;
            rows += 1;
        }
        out.send(Event::ResultEnd {
            index,
            result,
            rows: rows.to_string(),
            affected_rows: affected,
        })?;
        result += 1;
    }
    out.send(Event::StatementEnd { index })
}
fn binary(c: &MysqlColumn) -> bool {
    use mysql_async::consts::ColumnType::*;
    matches!(c.column_type(), MYSQL_TYPE_BIT | MYSQL_TYPE_GEOMETRY)
        || c.character_set() == 63
            && matches!(
                c.column_type(),
                MYSQL_TYPE_BLOB
                    | MYSQL_TYPE_TINY_BLOB
                    | MYSQL_TYPE_MEDIUM_BLOB
                    | MYSQL_TYPE_LONG_BLOB
                    | MYSQL_TYPE_STRING
                    | MYSQL_TYPE_VAR_STRING
                    | MYSQL_TYPE_VARCHAR
            )
}
fn encode(value: Value, column: &MysqlColumn) -> Result<Json> {
    Ok(match value {
        Value::NULL => Json::Null,
        Value::Bytes(v) if binary(column) => Json::String(STANDARD.encode(v)),
        Value::Bytes(v) => Json::String(String::from_utf8(v)?),
        Value::Int(v) => Json::String(v.to_string()),
        Value::UInt(v) => Json::String(v.to_string()),
        Value::Float(v) => Json::String(v.to_string()),
        Value::Double(v) => Json::String(v.to_string()),
        Value::Date(y, m, d, h, n, s, u) => Json::String(
            if column.column_type() == mysql_async::consts::ColumnType::MYSQL_TYPE_DATE {
                format!("{y:04}-{m:02}-{d:02}")
            } else {
                format!("{y:04}-{m:02}-{d:02}T{h:02}:{n:02}:{s:02}.{u:06}")
            },
        ),
        Value::Time(negative, days, h, m, s, u) => Json::String(format!(
            "{}{:02}:{m:02}:{s:02}.{u:06}",
            if negative { "-" } else { "" },
            u64::from(days) * 24 + u64::from(h)
        )),
    })
}

#[cfg(test)]
mod connection_tests {
    use super::*;

    #[test]
    fn refused_connection_identifies_database_endpoint() {
        let error = connection_error(
            "127.0.0.1",
            23306,
            io::Error::from(io::ErrorKind::ConnectionRefused).into(),
        );
        let message = error.to_string();
        assert!(message.contains("MySQL at 127.0.0.1:23306 refused the connection"));
        assert!(!message.contains("Input/output error"));
    }

    #[test]
    fn other_errors_preserve_driver_error_type() {
        let error = connection_error(
            "127.0.0.1",
            23306,
            io::Error::from(io::ErrorKind::ConnectionReset).into(),
        );
        assert!(error.downcast_ref::<mysql_async::Error>().is_some());
    }
}
