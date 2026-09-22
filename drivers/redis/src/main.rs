//! Redis worker: one connection per invocation, one command per statement.
use anyhow::{bail, Context, Result};
use redis::{aio::MultiplexedConnection, cmd, Client, Value as RedisValue};
use sqlx_protocol::{Action, Database, Emitter, Event, Request, VERSION};
use std::{io, time::Duration};

mod reply;
use reply::{map_reply, split_command, RedisCell};

#[tokio::main]
async fn main() {
    let mut out = Emitter::new(io::stdout());
    let result = async {
        out.send(Event::Ready {
            protocol_version: VERSION,
        })?;
        let req = Request::read()?;
        if !matches!(req.connection.database_type, Database::Redis) {
            bail!("wrong database worker");
        }
        match execute(&req, &mut out).await {
            Ok(()) => Ok(true),
            Err((index, error)) => {
                let code = error_code(&error);
                // A command the server rejected did not run; anything else may have run.
                let outcome = match (index, error.downcast_ref::<redis::RedisError>()) {
                    (None, _) => "not_started",
                    // The server answered with an error code, so the command did not run.
                    (Some(_), Some(redis_error)) if redis_error.code().is_some() => "failed",
                    (Some(_), _) => "unknown",
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
            eprintln!("Redis worker failed: {error}");
            std::process::exit(1);
        }
    }
}

/// Prefer the server's own error code, for example `WRONGTYPE`, over a generic one.
fn error_code(error: &anyhow::Error) -> String {
    match error.downcast_ref::<redis::RedisError>() {
        Some(redis_error) => {
            let suffix = redis_error
                .code()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{:?}", redis_error.kind()));
            format!("redis.{}", suffix.to_ascii_uppercase())
        }
        None => "redis.execution_failed".into(),
    }
}

/// Build the connection URL. Credentials are percent-encoded, and TLS switches the scheme.
fn connection_url(c: &sqlx_protocol::Connection) -> String {
    let scheme = if c.tls == "disable" { "redis" } else { "rediss" };
    let credentials = if c.username.is_empty() && c.password.is_empty() {
        String::new()
    } else if c.username.is_empty() {
        format!(":{}@", encode(&c.password))
    } else {
        format!("{}:{}@", encode(&c.username), encode(&c.password))
    };
    let database = c.database.trim();
    let path = if database.is_empty() || database == "0" {
        String::new()
    } else {
        format!("/{database}")
    };
    let host = if c.host.contains(':') {
        format!("[{}]", c.host)
    } else {
        c.host.clone()
    };
    format!("{scheme}://{credentials}{host}:{}{path}", c.port)
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn to_cell(value: &RedisValue) -> RedisCell {
    match value {
        RedisValue::Nil => RedisCell::Nil,
        RedisValue::Int(number) => RedisCell::Integer(*number),
        RedisValue::Okay => RedisCell::Status("OK".into()),
        RedisValue::SimpleString(text) => RedisCell::Text(text.clone()),
        RedisValue::Double(number) => RedisCell::Text(number.to_string()),
        RedisValue::Boolean(flag) => RedisCell::Text(flag.to_string()),
        RedisValue::VerbatimString { text, .. } => RedisCell::Text(text.clone()),
        RedisValue::BulkString(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => RedisCell::Text(text.to_owned()),
            Err(_) => RedisCell::Bytes(bytes.clone()),
        },
        RedisValue::Array(items) => RedisCell::Array(items.iter().map(to_cell).collect()),
        RedisValue::Set(items) => RedisCell::Array(items.iter().map(to_cell).collect()),
        RedisValue::Map(entries) => RedisCell::Array(
            entries
                .iter()
                .flat_map(|(key, value)| [to_cell(key), to_cell(value)])
                .collect(),
        ),
        other => RedisCell::Text(format!("{other:?}")),
    }
}

async fn connect(c: &sqlx_protocol::Connection) -> Result<MultiplexedConnection> {
    let client = Client::open(connection_url(c)).context("invalid Redis connection settings")?;
    let timeout = c
        .properties
        .get("connect_timeout_seconds")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(15);
    tokio::time::timeout(timeout_secs(timeout), client.get_multiplexed_async_connection())
        .await
        .context("Redis connection timed out")?
        .context("Redis connection failed")
}

fn timeout_secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds.max(1))
}

async fn execute(
    req: &Request,
    out: &mut Emitter<io::Stdout>,
) -> Result<(), (Option<usize>, anyhow::Error)> {
    let c = &req.connection;
    for key in c.properties.keys() {
        if key != "connect_timeout_seconds" {
            return Err((
                None,
                anyhow::anyhow!("unsupported Redis connection property: {key}"),
            ));
        }
    }
    let mut connection = connect(c).await.map_err(|error| (None, error))?;
    let ping: RedisValue = cmd("PING")
        .query_async(&mut connection)
        .await
        .map_err(|error: redis::RedisError| (None, error.into()))?;
    if !matches!(ping, RedisValue::Okay | RedisValue::SimpleString(_) | RedisValue::BulkString(_)) {
        return Err((None, anyhow::anyhow!("Redis did not answer PING")));
    }
    out.send(Event::Connected).map_err(|error| (None, error))?;
    if req.action == Action::Execute {
        for (index, statement) in req.statements.iter().enumerate() {
            run_command(&mut connection, out, index, statement)
                .await
                .map_err(|error| (Some(index), error))?;
        }
    }
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        cmd("QUIT").query_async::<()>(&mut connection),
    )
    .await;
    out.send(Event::Complete { success: true })
        .map_err(|error| (None, error))
}

async fn run_command(
    connection: &mut MultiplexedConnection,
    out: &mut Emitter<io::Stdout>,
    index: usize,
    statement: &str,
) -> Result<()> {
    let arguments = split_command(statement).map_err(anyhow::Error::msg)?;
    let mut command = cmd(&arguments[0]);
    for argument in &arguments[1..] {
        command.arg(argument);
    }
    out.send(Event::StatementStart { index })?;
    let reply: RedisValue = command.query_async(connection).await?;
    let cell = to_cell(&reply);
    let mapping = map_reply(&arguments[0], &cell);
    out.send(Event::Columns {
        index,
        result: 0,
        columns: mapping.columns,
    })?;
    for values in &mapping.rows {
        out.send(Event::Row {
            index,
            result: 0,
            values: values.clone(),
        })?;
    }
    out.send(Event::ResultEnd {
        index,
        result: 0,
        rows: mapping.rows.len().to_string(),
        affected_rows: None,
    })?;
    out.send(Event::StatementEnd { index })?;
    Ok(())
}
