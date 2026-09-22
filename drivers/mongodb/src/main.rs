//! MongoDB worker: one connection per invocation, one JSON command per statement.
use anyhow::{bail, Context, Result};
use mongodb::{
    bson::{doc, Document},
    Client, Database as MongoDatabase,
};
use sqlx_protocol::{Action, Connection, Database, Emitter, Event, Request, VERSION};
use std::{fmt, io, time::Duration};

mod reply;
use reply::BatchFetcher;

/// How long the worker waits for the driver to reach the deployment.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[tokio::main]
async fn main() {
    let mut out = Emitter::new(io::stdout());
    let result: Result<bool> = async {
        out.send(Event::Ready {
            protocol_version: VERSION,
        })?;
        let request = Request::read()?;
        if !matches!(request.connection.database_type, Database::Mongodb) {
            bail!("wrong database worker");
        }
        match execute(&request, &mut out).await {
            Ok(()) => Ok(true),
            Err((index, error)) => {
                out.failed(
                    &request,
                    index,
                    &failure_code(&error),
                    &failure_message(&error),
                    failure_outcome(index, &error),
                )?;
                Ok(false)
            }
        }
    }
    .await;
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("MongoDB worker failed: {error}");
            std::process::exit(1);
        }
    }
}

/// A failure the worker classifies itself: a statement that is not a command, or a reply the
/// server rejected.
#[derive(Debug)]
struct Failure {
    code: String,
    message: String,
    outcome: &'static str,
}
impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}

fn failed(code: impl Into<String>, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(Failure {
        code: code.into(),
        message: message.into(),
        outcome: "failed",
    })
}

/// `mongodb.<codeName>` when the server named the code, else `mongodb.<code>`, else
/// `mongodb.execution_failed`.
fn failure_code(error: &anyhow::Error) -> String {
    if let Some(failure) = error.downcast_ref::<Failure>() {
        return failure.code.clone();
    }
    match command_error(error) {
        Some(command) if !command.code_name.is_empty() => format!("mongodb.{}", command.code_name),
        Some(command) => format!("mongodb.{}", command.code),
        None => "mongodb.execution_failed".into(),
    }
}

/// Keep the server's own message instead of the driver's wrapper around it.
fn failure_message(error: &anyhow::Error) -> String {
    if let Some(failure) = error.downcast_ref::<Failure>() {
        return failure.message.clone();
    }
    match command_error(error) {
        Some(command) => command.message.clone(),
        None => error.to_string(),
    }
}

/// A connection-level failure was never dispatched, a rejection proves the command did not run,
/// and anything else may have run before the connection went away.
fn failure_outcome(index: Option<usize>, error: &anyhow::Error) -> &'static str {
    if let Some(failure) = error.downcast_ref::<Failure>() {
        return failure.outcome;
    }
    if index.is_none() {
        return "not_started";
    }
    let Some(error) = error.downcast_ref::<mongodb::error::Error>() else {
        return "unknown";
    };
    match error.kind.as_ref() {
        mongodb::error::ErrorKind::Command(_)
        | mongodb::error::ErrorKind::InvalidArgument { .. }
        | mongodb::error::ErrorKind::BsonSerialization(_) => "failed",
        _ => "unknown",
    }
}

fn command_error(error: &anyhow::Error) -> Option<&mongodb::error::CommandError> {
    match error.downcast_ref::<mongodb::error::Error>()?.kind.as_ref() {
        mongodb::error::ErrorKind::Command(command) => Some(command),
        _ => None,
    }
}

async fn execute(
    request: &Request,
    out: &mut Emitter<io::Stdout>,
) -> Result<(), (Option<usize>, anyhow::Error)> {
    let connection = &request.connection;
    let client = connect(connection).await.map_err(|error| (None, error))?;
    let database = client.database(database_name(connection));
    database
        .run_command(doc! { "ping": 1 })
        .await
        .context("MongoDB did not answer the ping")
        .map_err(|error| (None, error))?;
    out.send(Event::Connected).map_err(|error| (None, error))?;
    if request.action == Action::Execute {
        for (index, statement) in request.statements.iter().enumerate() {
            run_statement(&database, out, index, statement)
                .await
                .map_err(|error| (Some(index), error))?;
        }
    }
    Ok(())
}

async fn run_statement(
    database: &MongoDatabase,
    out: &mut Emitter<io::Stdout>,
    index: usize,
    statement: &str,
) -> Result<()> {
    out.send(Event::StatementStart { index })?;
    let command = parse_command(statement)?;
    let result = database.run_command(command).await?;
    if let Some(rejected) = reply::rejection(&result) {
        return Err(failed(rejected.code, rejected.message));
    }
    let reply::Mapping {
        columns,
        rows,
        affected_rows,
    } = reply::map_reply(&result, &DatabaseFetcher { database }).await?;
    let count = rows.len().to_string();
    out.send(Event::Columns {
        index,
        result: 0,
        columns,
    })?;
    for values in rows {
        out.send(Event::Row {
            index,
            result: 0,
            values,
        })?;
    }
    out.send(Event::ResultEnd {
        index,
        result: 0,
        rows: count,
        affected_rows,
    })?;
    out.send(Event::StatementEnd { index })
}

/// Follows a cursor over the same connection: `{ "getMore": <id>, "collection": <name> }`.
struct DatabaseFetcher<'a> {
    database: &'a MongoDatabase,
}
impl BatchFetcher for DatabaseFetcher<'_> {
    async fn fetch(&self, cursor_id: i64, collection: &str) -> Result<Document> {
        self.database
            .run_command(doc! { "getMore": cursor_id, "collection": collection })
            .await
            .map_err(Into::into)
    }
}

/// Parse one statement as the JSON command object `db.runCommand()` takes.
fn parse_command(statement: &str) -> Result<Document> {
    let example = r#"expected a JSON command object such as {"find":"users","filter":{}}"#;
    let value: serde_json::Value = serde_json::from_str(statement).map_err(|error| {
        failed(
            "mongodb.invalid_command",
            format!("{example}, but the statement is not valid JSON: {error}"),
        )
    })?;
    let serde_json::Value::Object(object) = value else {
        return Err(failed(
            "mongodb.invalid_command",
            format!("{example}, but the statement is {}", kind(&value)),
        ));
    };
    mongodb::bson::to_document(&object).map_err(|error| {
        failed(
            "mongodb.invalid_command",
            format!("{example}, but the statement is not valid BSON: {error}"),
        )
    })
}

/// The JSON kind of a value, for the message that rejects a statement that is not an object.
fn kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Connect with a deadline: the driver reaches the deployment while it parses the URI.
async fn connect(connection: &Connection) -> Result<Client> {
    let uri = connection_uri(connection)?;
    tokio::time::timeout(CONNECT_TIMEOUT, Client::with_uri_str(&uri))
        .await
        .context("MongoDB connection timed out")?
        .context("MongoDB connection failed")
}

/// MongoDB has no default database, so an empty selection runs against `admin`.
fn database_name(connection: &Connection) -> &str {
    if connection.database.trim().is_empty() {
        "admin"
    } else {
        &connection.database
    }
}

/// Build the connection URI: credentials, host, optional database, then the query options.
///
/// `properties.atlas` switches to `mongodb+srv`, `properties.authSource` replaces the default
/// `admin` source, TLS adds `tls=true`, and every other property passes through as an option.
fn connection_uri(connection: &Connection) -> Result<String> {
    let properties = &connection.properties;
    let atlas = properties
        .get("atlas")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let scheme = if atlas { "mongodb+srv" } else { "mongodb" };
    let credentials = if connection.username.is_empty() {
        String::new()
    } else {
        format!(
            "{}:{}@",
            encode(&connection.username),
            encode(&connection.password)
        )
    };
    let database = if connection.database.trim().is_empty() {
        String::new()
    } else {
        format!("/{}", connection.database)
    };
    let host = if connection.host.contains(':') {
        format!("[{}]", connection.host)
    } else {
        connection.host.clone()
    };
    let source = properties
        .get("authSource")
        .cloned()
        .or_else(|| (!connection.username.is_empty()).then(|| "admin".to_owned()));
    let mut query = Vec::new();
    if let Some(source) = source {
        query.push(("authSource", source));
    }
    if connection.tls != "disable" {
        query.push(("tls", "true".to_owned()));
    }
    for (key, value) in properties {
        if key == "atlas" || key == "authSource" {
            continue;
        }
        if !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            bail!("unsupported MongoDB connection property: {key}");
        }
        query.push((key.as_str(), value.clone()));
    }
    let query: Vec<String> = query
        .into_iter()
        .map(|(key, value)| format!("{key}={}", encode(&value)))
        .collect();
    let query = if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query.join("&"))
    };
    Ok(format!(
        "{scheme}://{credentials}{host}:{}{database}{query}",
        connection.port
    ))
}

/// Percent-encode a URI component. Unreserved characters stay, everything else becomes `%XX`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn connection() -> Connection {
        Connection {
            database_type: Database::Mongodb,
            host: "127.0.0.1".into(),
            port: 37017,
            database: "sqlx_probe".into(),
            service: String::new(),
            username: "root".into(),
            password: "sqlx_test_only_password".into(),
            tls: "disable".into(),
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn builds_a_plain_uri_with_credentials_and_auth_source() {
        assert_eq!(
            connection_uri(&connection()).unwrap(),
            "mongodb://root:sqlx_test_only_password@127.0.0.1:37017/sqlx_probe?authSource=admin"
        );
    }

    #[test]
    fn encodes_credentials_and_rejects_an_unusable_key() {
        let mut connection = connection();
        connection.username = "user@example.com".into();
        connection.password = "p@ss/word".into();
        connection.tls = "verify-full".into();
        assert_eq!(
            connection_uri(&connection).unwrap(),
            "mongodb://user%40example.com:p%40ss%2Fword@127.0.0.1:37017/sqlx_probe?authSource=admin&tls=true"
        );
        connection.properties.insert("bad key".into(), "1".into());
        assert!(connection_uri(&connection).is_err());
    }

    #[test]
    fn builds_an_atlas_uri_and_passes_properties_through() {
        let mut connection = connection();
        connection.tls = "verify-full".into();
        connection.properties.insert("atlas".into(), "true".into());
        connection
            .properties
            .insert("authSource".into(), "sqlx_probe".into());
        connection
            .properties
            .insert("retryWrites".into(), "false".into());
        assert_eq!(
            connection_uri(&connection).unwrap(),
            "mongodb+srv://root:sqlx_test_only_password@127.0.0.1:37017/sqlx_probe?authSource=sqlx_probe&tls=true&retryWrites=false"
        );
    }

    #[test]
    fn omits_credentials_auth_source_and_an_empty_database() {
        let mut connection = connection();
        connection.username.clear();
        connection.password.clear();
        assert_eq!(
            connection_uri(&connection).unwrap(),
            "mongodb://127.0.0.1:37017/sqlx_probe"
        );
        connection.username = "root".into();
        connection.database.clear();
        assert_eq!(
            connection_uri(&connection).unwrap(),
            "mongodb://root:sqlx_test_only_password@127.0.0.1:37017?authSource=admin"
        );
        assert_eq!(database_name(&connection), "admin");
    }

    #[test]
    fn rejects_a_statement_that_is_not_a_command_object() {
        let error = parse_command("not json").unwrap_err();
        assert!(error.to_string().contains("expected a JSON command object"));
        assert!(parse_command("[]").is_err());
        assert_eq!(
            parse_command(r#"{"find":"users"}"#)
                .unwrap()
                .get_str("find")
                .unwrap(),
            "users"
        );
    }
}
