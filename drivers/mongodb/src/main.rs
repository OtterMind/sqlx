//! MongoDB worker: one connection per invocation, one JSON command per statement.
use anyhow::{bail, Context, Result};
use mongodb::{
    bson::{doc, Bson, Document},
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

/// Keep the server's own message instead of the driver's wrapper around it, and keep the whole
/// cause chain of a failure that has no server message.
fn failure_message(error: &anyhow::Error) -> String {
    if let Some(failure) = error.downcast_ref::<Failure>() {
        return failure.message.clone();
    }
    match command_error(error) {
        Some(command) => command.message.clone(),
        None => format!("{error:#}"),
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

/// The server's own error. A wrapper such as the SCRAM handshake keeps it as its source, so the
/// chain is searched too; without that an authentication failure would lose its `codeName`.
fn command_error(error: &anyhow::Error) -> Option<&mongodb::error::CommandError> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error.as_ref());
    while let Some(source) = current {
        if let Some(command) = server_command_error(source) {
            return Some(command);
        }
        current = source.source();
    }
    None
}

/// The command error of one link in the chain. A boxed link is unwrapped: the driver's own
/// `source` field is an `Option<Box<Error>>`, which thiserror hands over as the box itself.
fn server_command_error<'a>(
    source: &'a (dyn std::error::Error + 'static),
) -> Option<&'a mongodb::error::CommandError> {
    let error = match source.downcast_ref::<mongodb::error::Error>() {
        Some(error) => error,
        None => source
            .downcast_ref::<Box<mongodb::error::Error>>()?
            .as_ref(),
    };
    match error.kind.as_ref() {
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
    out.send(Event::Complete { success: true })
        .map_err(|error| (None, error))
}

async fn run_statement(
    database: &MongoDatabase,
    out: &mut Emitter<io::Stdout>,
    index: usize,
    statement: &str,
) -> Result<()> {
    out.send(Event::StatementStart { index })?;
    let parsed = parse_command(statement)?;
    let name = parsed.keys().next().cloned().unwrap_or_default();
    let command = server_command(parsed);
    let result = database.run_command(command).await?;
    if let Some(rejected) = reply::rejection(&result) {
        return Err(failed(rejected.code, rejected.message));
    }
    let mapping = match reply::named_mapping(&name, &result) {
        Some(mapping) => mapping,
        None => reply::map_reply(&result, &DatabaseFetcher { database }).await?,
    };
    let reply::Mapping {
        columns,
        rows,
        affected_rows,
    } = mapping;
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

/// Rewrite the driver-style write helpers onto the server commands.
///
/// `insertOne`, `updateMany` and friends are client helpers, not server commands: the server only
/// knows `insert`, `update` and `delete`. `{"insertOne":"users","document":{...}}` therefore
/// becomes `{"insert":"users","documents":[{...}]}`, and `{"deleteMany":"users","filter":{}}`
/// becomes `{"delete":"users","deletes":[{"q":{},"limit":0}]}`. Every other statement passes
/// through untouched, so the server commands stay available as they are.
fn server_command(command: Document) -> Document {
    let Some((name, collection)) = collection_command(&command) else {
        return command;
    };
    rewrite(&name, &collection, &command).unwrap_or(command)
}

/// The command name and its collection, when the first field names a collection.
fn collection_command(command: &Document) -> Option<(String, String)> {
    let (name, collection) = command.iter().next()?;
    match collection {
        Bson::String(collection) => Some((name.clone(), collection.clone())),
        _ => None,
    }
}

/// Build the server command for one write helper. The command name stays first: the server reads
/// the command name from it. `None` leaves a server command untouched.
fn rewrite(name: &str, collection: &str, command: &Document) -> Option<Document> {
    let mut arguments: Document = command
        .iter()
        .skip(1)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let rewritten = match name {
        // findOne is the shell's single-document read; the server command is find with a limit.
        "findOne" => {
            arguments.insert("limit", 1);
            doc! { "find": collection, "filter": arguments.get("filter").cloned().unwrap_or(Bson::Document(Document::new())) }
                .into_iter()
                .chain(arguments.iter().filter(|(key, _)| key.as_str() != "filter").map(|(key, value)| (key.clone(), value.clone())))
                .collect()
        }
        "insertOne" | "insertMany" => {
            let documents = if name == "insertOne" {
                vec![arguments.remove("document").unwrap_or(Bson::Null)]
            } else {
                match arguments.remove("documents") {
                    Some(Bson::Array(documents)) => documents,
                    _ => Vec::new(),
                }
            };
            let mut insert = Document::new();
            insert.insert("insert", collection);
            insert.insert("documents", documents);
            take_all(
                &mut insert,
                &mut arguments,
                [
                    "ordered",
                    "bypassDocumentValidation",
                    "writeConcern",
                    "comment",
                    "let",
                ],
            );
            insert
        }
        "updateOne" | "updateMany" | "replaceOne" => {
            let mut entry = Document::new();
            entry.insert("q", filter(&mut arguments));
            entry.insert(
                "u",
                arguments
                    .remove(if name == "replaceOne" {
                        "replacement"
                    } else {
                        "update"
                    })
                    .unwrap_or(Bson::Null),
            );
            if name == "updateMany" {
                entry.insert("multi", true);
            }
            take_all(
                &mut entry,
                &mut arguments,
                ["upsert", "collation", "arrayFilters", "hint"],
            );
            let mut update = Document::new();
            update.insert("update", collection);
            update.insert("updates", vec![entry]);
            take_all(
                &mut update,
                &mut arguments,
                [
                    "ordered",
                    "writeConcern",
                    "bypassDocumentValidation",
                    "comment",
                    "let",
                ],
            );
            update
        }
        "deleteOne" | "deleteMany" => {
            let mut entry = Document::new();
            entry.insert("q", filter(&mut arguments));
            entry.insert("limit", if name == "deleteOne" { 1_i32 } else { 0_i32 });
            take_all(&mut entry, &mut arguments, ["collation", "hint"]);
            let mut delete = Document::new();
            delete.insert("delete", collection);
            delete.insert("deletes", vec![entry]);
            take_all(
                &mut delete,
                &mut arguments,
                ["ordered", "writeConcern", "comment", "let"],
            );
            delete
        }
        _ => return None,
    };
    Some(rewritten)
}

/// The helper's `filter`, or an empty filter that matches every document.
fn filter(arguments: &mut Document) -> Bson {
    arguments
        .remove("filter")
        .unwrap_or_else(|| Bson::Document(Document::new()))
}

/// Move the fields the caller set from the helper's arguments onto the server command.
fn take_all<const N: usize>(target: &mut Document, arguments: &mut Document, keys: [&str; N]) {
    for key in keys {
        if let Some(value) = arguments.remove(key) {
            target.insert(key, value);
        }
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
    // The driver rejects any port in a `mongodb+srv` URI: an Atlas URI takes its hosts from the
    // SRV record instead of the connection fields.
    let authority = if atlas {
        format!("{credentials}{host}")
    } else {
        format!("{credentials}{host}:{}", connection.port)
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
    Ok(format!("{scheme}://{authority}{database}{query}"))
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
    fn builds_an_atlas_uri_without_a_port_and_passes_properties_through() {
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
            "mongodb+srv://root:sqlx_test_only_password@127.0.0.1/sqlx_probe?authSource=sqlx_probe&tls=true&retryWrites=false"
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
        connection.password = "sqlx_test_only_password".into();
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

    #[test]
    fn rewrites_driver_write_helpers_onto_server_commands() {
        assert_eq!(
            server_command(parse_command(r#"{"insertOne":"users","document":{"_id":1}}"#).unwrap()),
            doc! {"insert": "users", "documents": [{"_id": 1_i64}]}
        );
        assert_eq!(
            server_command(
                parse_command(
                    r#"{"insertMany":"users","documents":[{"_id":1},{"_id":2}],"ordered":false}"#
                )
                .unwrap()
            ),
            doc! {"insert": "users", "documents": [{"_id": 1_i64}, {"_id": 2_i64}], "ordered": false}
        );
        assert_eq!(
            server_command(
                parse_command(
                    r#"{"updateMany":"users","filter":{"a":1},"update":{"$set":{"b":2}},"upsert":true}"#
                )
                .unwrap()
            ),
            doc! {"update": "users", "updates": [
                {"q": {"a": 1_i64}, "u": {"$set": {"b": 2_i64}}, "multi": true, "upsert": true},
            ]}
        );
        assert_eq!(
            server_command(
                parse_command(r#"{"replaceOne":"users","filter":{"a":1},"replacement":{"b":2}}"#)
                    .unwrap()
            ),
            doc! {"update": "users", "updates": [{"q": {"a": 1_i64}, "u": {"b": 2_i64}}]}
        );
        assert_eq!(
            server_command(parse_command(r#"{"deleteMany":"users","filter":{}}"#).unwrap()),
            doc! {"delete": "users", "deletes": [{"q": {}, "limit": 0_i32}]}
        );
        assert_eq!(
            server_command(parse_command(r#"{"deleteOne":"users"}"#).unwrap()),
            doc! {"delete": "users", "deletes": [{"q": {}, "limit": 1_i32}]}
        );
    }

    #[test]
    fn passes_server_commands_through_with_the_name_first() {
        let find = parse_command(r#"{"find":"users","filter":{},"limit":5}"#).unwrap();
        assert_eq!(find.keys().next().unwrap().as_str(), "find");
        assert_eq!(server_command(find.clone()), find);
    }
}
