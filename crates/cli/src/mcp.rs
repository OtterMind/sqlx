//! Minimal Model Context Protocol server over stdio, exposing SQLX tools to agent harnesses.
//!
//! The harness (Claude Code, Codex, or a thin native-tool shim) launches `sqlx mcp`; SQLX keeps
//! protocol output on stdout, so execution results are rendered into the tool response instead of
//! being streamed as CLI JSON. Downloads and worker diagnostics still go to stderr.
use crate::{
    execution,
    output::{self, Mode},
    prefetch, results, settings,
    storage::{Datasource, Store},
    ui,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sqlx_protocol::{Action, Connection, Event};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::{mpsc::Sender, Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

/// Protocol revision used when the client does not request one.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// What a client that installed no Skill is told about using this server.
const INSTRUCTIONS: &str = "SQLX runs SQL against databases the user already saved. Start with \
sqlx_datasource_list, then pass a datasource's ID or unique name to sqlx_sql_execute. The user \
creates, tests and edits datasources with the sqlx CLI and types passwords in a local browser page, \
so never ask for credentials in the conversation. Run a statement that changes data only after the \
user authorized that exact operation and scope, and never replay one whose outcome is unknown. A \
result set larger than the preview is stored and read back with sqlx_results_rows; use \
sqlx_sql_view when the user should see the rows in a browser page.";
/// One message read from the client.
enum Job {
    /// A message that needs an answer; `cancel` is set for `tools/call` so a later cancellation
    /// notification can reach the running call.
    Request {
        request: Value,
        cancel: Option<CancellationToken>,
    },
    /// A response the reader built itself, such as a parse error.
    Response(Value),
}
/// Every tool call the client has asked for and this process has not answered yet. A client may
/// issue several before the first finishes, and each one stays cancellable on its own.
#[derive(Default)]
struct InFlight {
    running: Mutex<BTreeMap<String, CancellationToken>>,
}
impl InFlight {
    fn start(&self, id: &Value, cancel: CancellationToken) {
        self.running.lock().unwrap().insert(id.to_string(), cancel);
    }
    /// Cancels the call the client named; an unknown or finished one is ignored.
    fn cancel(&self, id: &Value) {
        if let Some(cancel) = self.running.lock().unwrap().get(&id.to_string()) {
            cancel.cancel();
        }
    }
    fn finish(&self, id: &Value) {
        self.running.lock().unwrap().remove(&id.to_string());
    }
}
pub fn serve(root: PathBuf, manifest: String, local: Option<PathBuf>) -> Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let in_flight = Arc::new(InFlight::default());
    let (sender, receiver) = std::sync::mpsc::channel::<Job>();
    {
        let in_flight = Arc::clone(&in_flight);
        std::thread::spawn(move || read_messages(sender, &in_flight));
    }
    // Only this loop writes to stdout, so a progress notification and the response that follows it
    // never interleave. The reader keeps the client's cancellation reachable while a tool runs.
    for job in receiver {
        match job {
            Job::Response(response) => respond(&mut output, &response)?,
            Job::Request { request, cancel } => {
                let id = request.get("id").cloned();
                if let Some(response) = handle(
                    &request,
                    &root,
                    &manifest,
                    &local,
                    &mut output,
                    cancel.as_ref(),
                ) {
                    respond(&mut output, &response)?;
                }
                if let Some(id) = id {
                    in_flight.finish(&id);
                }
            }
        }
    }
    Ok(())
}
/// Reads client messages for as long as the process lives. Notifications are handled here so that
/// `notifications/cancelled` stops a running tool instead of waiting behind it.
fn read_messages(sender: Sender<Job>, in_flight: &InFlight) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let _ = sender.send(Job::Response(json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": format!("parse error: {error}")},
                })));
                continue;
            }
        };
        let Some(id) = request.get("id").cloned() else {
            // A notification is never answered; a cancellation is the one that changes anything.
            if request["method"].as_str() == Some("notifications/cancelled") {
                if let Some(request_id) = request
                    .get("params")
                    .and_then(|params| params.get("requestId"))
                {
                    in_flight.cancel(request_id);
                }
            }
            continue;
        };
        let cancel = (request["method"].as_str() == Some("tools/call")).then(|| {
            let token = CancellationToken::new();
            in_flight.start(&id, token.clone());
            token
        });
        if sender.send(Job::Request { request, cancel }).is_err() {
            break;
        }
    }
}
fn respond(output: &mut dyn Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
/// Reports the steps of a running tool call to a client that asked for progress.
struct Progress<'a> {
    token: Option<Value>,
    output: &'a mut dyn Write,
}
impl<'a> Progress<'a> {
    /// A call made without a progress token stays silent, which is what most clients send.
    fn from_params(params: &Value, output: &'a mut dyn Write) -> Self {
        let token = params
            .get("_meta")
            .and_then(|meta| meta.get("progressToken"))
            .cloned();
        Self { token, output }
    }
    fn report(&mut self, progress: u64, total: Option<u64>, message: &str) {
        let Some(token) = self.token.clone() else {
            return;
        };
        let mut params = json!({"progressToken": token, "progress": progress});
        if let Some(total) = total {
            params["total"] = json!(total);
        }
        if !message.is_empty() {
            params["message"] = json!(message);
        }
        let _ = respond(
            self.output,
            &json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": params}),
        );
    }
}
/// Handle one JSON-RPC message. Notifications (no `id`) produce no response.
fn handle(
    request: &Value,
    root: &Path,
    manifest: &str,
    local: &Option<PathBuf>,
    output: &mut dyn Write,
    cancel: Option<&CancellationToken>,
) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request["method"].as_str().unwrap_or_default();
    id.as_ref()?;
    let id = id.unwrap_or(Value::Null);
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {
                "name": "sqlx",
                "title": "OtterMind SQLX",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let mut progress = Progress::from_params(&params, output);
            call(params, root, manifest, local, &mut progress, cancel)
        }
        other => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {other}")},
            }))
        }
    };
    Some(match result {
        Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{"type": "text", "text": format!("{error:#}")}],
                "isError": true,
            },
        }),
    })
}
fn call(
    params: Value,
    root: &Path,
    manifest: &str,
    local: &Option<PathBuf>,
    progress: &mut Progress<'_>,
    cancel: Option<&CancellationToken>,
) -> Result<Value> {
    let name = params["name"].as_str().context("missing tool name")?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let value = match name {
        "sqlx_datasource_list" => {
            let store = Store::open(root.to_path_buf())?;
            json!({"datasources": store.load()?.iter().map(Datasource::public).collect::<Vec<_>>()})
        }
        "sqlx_datasource_show" => {
            let id = string_arg(&arguments, "id")?;
            Store::open(root.to_path_buf())?.find(&id)?.public()
        }
        "sqlx_datasource_test" => {
            let id = string_arg(&arguments, "id")?;
            let source = Store::open(root.to_path_buf())?.find(&id)?;
            run_action(
                root,
                manifest,
                local,
                source,
                Action::Test,
                vec![],
                progress,
                cancel,
            )?
        }
        "sqlx_datasource_setup" => {
            let name = string_arg(&arguments, "name")?;
            let connection: Connection = serde_json::from_value(
                arguments
                    .get("connection")
                    .cloned()
                    .context("missing connection object")?,
            )
            .context("connection must be the object `--connection-stdin` accepts")?;
            let client =
                ui::UiClient::start(root.to_path_buf(), manifest.to_owned(), local.clone())?;
            let mut result = client.post(
                "/api/setups",
                &ui::SetupRequest {
                    name,
                    source_id: None,
                    connection,
                },
            )?;
            result["url"] = client
                .page(
                    &format!(
                        "/setup/{}",
                        result["request_id"]
                            .as_str()
                            .context("missing request id")?
                    ),
                    false,
                )?
                .into();
            result
        }
        "sqlx_datasource_setup_status" => {
            let request_id = string_arg(&arguments, "request_id")?;
            uuid::Uuid::parse_str(&request_id)
                .context("request_id must be a setup request UUID")?;
            let client = ui::UiClient::existing(root)?
                .context("local UI is not running; the setup request has expired")?;
            client.get(&format!("/api/setups/{request_id}/status"))?
        }
        "sqlx_sql_execute" => {
            let source = datasource_arg(root, &arguments)?;
            let statements = statements_arg(&arguments)?;
            let full = arguments
                .get("full")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            run_action_with_mode(
                root,
                manifest,
                local,
                source,
                Action::Execute,
                statements,
                full,
                progress,
                cancel,
            )?
        }
        "sqlx_sql_view" => {
            let source = datasource_arg(root, &arguments)?;
            let statements = statements_arg(&arguments)?;
            let client =
                ui::UiClient::start(root.to_path_buf(), manifest.to_owned(), local.clone())?;
            let mut result = client.post(
                "/api/results",
                &ui::ViewRequest {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    datasource: source.id,
                    statements,
                    description: None,
                },
            )?;
            result["url"] = client
                .page(
                    &format!(
                        "/result/{}",
                        result["result_id"].as_str().context("missing result id")?
                    ),
                    false,
                )?
                .into();
            result
        }
        "sqlx_results_rows" => stored_rows(root, &arguments)?,
        "sqlx_prefetch" => {
            let components = arguments
                .get("components")
                .and_then(Value::as_array)
                .context("components must be an array")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .context("components must be strings")
                })
                .collect::<Result<Vec<_>>>()?;
            let total = components.len();
            let mut observer = |component: &str, index: usize| {
                progress.report(
                    index as u64,
                    Some(total as u64),
                    &format!("fetching {component}"),
                );
            };
            let (report, complete) =
                prefetch::run(root, manifest, &components, Some(&mut observer))?;
            progress.report(total as u64, Some(total as u64), "components ready");
            json!({"complete": complete, "report": report})
        }
        other => bail!("unknown tool {other}"),
    };
    Ok(json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&value)?}],
        "isError": false,
    }))
}
#[allow(clippy::too_many_arguments)]
fn run_action(
    root: &Path,
    manifest: &str,
    local: &Option<PathBuf>,
    source: Datasource,
    action: Action,
    statements: Vec<String>,
    progress: &mut Progress<'_>,
    cancel: Option<&CancellationToken>,
) -> Result<Value> {
    run_action_with_mode(
        root, manifest, local, source, action, statements, false, progress, cancel,
    )
}
#[allow(clippy::too_many_arguments)]
fn run_action_with_mode(
    root: &Path,
    manifest: &str,
    local: &Option<PathBuf>,
    source: Datasource,
    action: Action,
    statements: Vec<String>,
    full: bool,
    progress: &mut Progress<'_>,
    cancel: Option<&CancellationToken>,
) -> Result<Value> {
    let flag = full.then_some(settings::ResultMode::Full);
    let (out, effective) = execution::output_settings(root, Mode::Compact, None, flag)?;
    let mut buffer = Vec::new();
    let total = statements.len();
    let mut report = |event: &Event| {
        if let Event::StatementStart { index } = event {
            progress.report(
                *index as u64,
                Some(total as u64),
                &format!("running statement {} of {total}", index + 1),
            );
        }
    };
    let observer = execution::Observer {
        cancel: cancel.cloned().unwrap_or_default(),
        event: Some(&mut report),
    };
    let outcome = execution::run_to(
        &mut buffer,
        root.to_path_buf(),
        manifest.to_owned(),
        local.clone(),
        source,
        action,
        statements,
        out,
        observer,
    );
    let pruned = execution::prune_results(&effective);
    let success = outcome?;
    pruned?;
    let value: Value =
        serde_json::from_slice(&buffer).context("execution returned invalid JSON")?;
    if value.get("success").and_then(Value::as_bool) != Some(success) {
        bail!("execution returned an unexpected result object");
    }
    Ok(value)
}
/// Read one page of a stored result back for harnesses without file access.
fn stored_rows(root: &Path, arguments: &Value) -> Result<Value> {
    let id = string_arg(arguments, "id")?;
    uuid::Uuid::parse_str(&id).context("id must be a result UUID")?;
    let (_, effective) = execution::output_settings(root, Mode::Compact, None, None)?;
    let path = effective.results_dir.join(&id);
    let store = results::ResultStore::recover(&path).with_context(|| {
        format!(
            "result {id} is not stored in {}",
            effective.results_dir.display()
        )
    })?;
    let statement = number_arg(arguments, "statement")?.unwrap_or(0) as usize;
    let set = number_arg(arguments, "set")?.unwrap_or(0) as usize;
    let offset = number_arg(arguments, "offset")?.unwrap_or(0);
    let limit = number_arg(arguments, "limit")?.unwrap_or(50).clamp(1, 200) as usize;
    let page = store.page(statement, set, offset, limit)?;
    let columns = store
        .metadata
        .tables
        .iter()
        .find(|table| table.statement == statement && table.result == set)
        .map(|table| {
            table
                .columns
                .iter()
                .map(output::column_json)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "cols": columns,
        "rows": page.rows,
        "offset": page.offset.to_string(),
        "next_offset": page.next_offset.to_string(),
        "complete": page.complete,
    }))
}
fn number_arg(arguments: &Value, name: &str) -> Result<Option<u64>> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .with_context(|| format!("{name} must be a whole number"))
            .map(Some),
    }
}
fn string_arg(arguments: &Value, name: &str) -> Result<String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string argument {name}"))
        .map(str::to_owned)
}
fn statements_arg(arguments: &Value) -> Result<Vec<String>> {
    let statements = arguments
        .get("statements")
        .and_then(Value::as_array)
        .context("statements must be an array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("statements must be strings")
        })
        .collect::<Result<Vec<_>>>()?;
    if statements
        .iter()
        .any(|statement| statement.trim().is_empty())
    {
        bail!("SQL statements must not be empty");
    }
    Ok(statements)
}
fn datasource_arg(root: &Path, arguments: &Value) -> Result<Datasource> {
    let id = string_arg(arguments, "datasource")?;
    Store::open(root.to_path_buf())?.find(&id)
}
fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "sqlx_datasource_list",
            "title": "List SQLX datasources",
            "description": "List the saved SQLX datasources with their non-secret connection settings. Never returns usernames or passwords.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
        }),
        json!({
            "name": "sqlx_datasource_show",
            "title": "Show a SQLX datasource",
            "description": "Show one saved datasource by its stable ID or unique name.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string", "description": "Datasource UUID or unique name"}},
                "required": ["id"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
        }),
        json!({
            "name": "sqlx_datasource_test",
            "title": "Test a SQLX connection",
            "description": "Open one connection and report whether the datasource is reachable. Executes no SQL.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string", "description": "Datasource UUID or unique name"}},
                "required": ["id"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": true},
        }),
        json!({
            "name": "sqlx_datasource_setup",
            "title": "Add a SQLX datasource in the browser",
            "description": "Start adding a datasource the user finishes in a local browser page, and return that page's `url`. The connection object carries no credentials: the user types the username and password there, so they never enter this conversation. Give the user the complete URL, then call sqlx_datasource_setup_status with the returned `request_id` until it reports the outcome.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Name for the new datasource"},
                    "connection": {
                        "type": "object",
                        "description": "Connection settings without credentials, in the shape --connection-stdin accepts: database_type, host, port, database, service, tls, properties",
                        "required": ["database_type"],
                        "additionalProperties": true,
                    },
                },
                "required": ["name", "connection"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "openWorldHint": true},
        }),
        json!({
            "name": "sqlx_datasource_setup_status",
            "title": "Read a browser setup request",
            "description": "Report the outcome of a setup started by sqlx_datasource_setup, without retrieving credentials. Read-only: it never contacts the database again.",
            "inputSchema": {
                "type": "object",
                "properties": {"request_id": {"type": "string", "description": "Setup request UUID from the sqlx_datasource_setup response"}},
                "required": ["request_id"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
        }),
        json!({
            "name": "sqlx_sql_execute",
            "title": "Execute SQL with SQLX",
            "description": "Execute one or more complete SQL statements through SQLX and return the table-shaped result: `results[].cols`, `rows` and `count` per statement. A result set larger than the preview is stored under `results[].file` and read with sqlx_results_rows. Each statement is a separate driver statement and the batch stops at the first error. Statements may modify data: only call this after the user authorized that exact operation and scope. Results are never replayed automatically.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "datasource": {"type": "string", "description": "Datasource UUID or unique name"},
                    "statements": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1,
                        "description": "Complete SQL statements, executed in order on one connection",
                    },
                    "full": {
                        "type": "boolean",
                        "description": "Print every row instead of a preview and store nothing. Use it only when the caller cannot read the stored file, because a large result then fills the response.",
                    },
                },
                "required": ["datasource", "statements"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "openWorldHint": true},
        }),
        json!({
            "name": "sqlx_sql_view",
            "title": "Show SQL results in the local page",
            "description": "Execute the statements once in the local SQLX service and return a local result-page URL for the user. The same authorization rule as sqlx_sql_execute applies, and the page's Refresh action reruns the batch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "datasource": {"type": "string", "description": "Datasource UUID or unique name"},
                    "statements": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1,
                        "description": "Complete SQL statements, executed in order on one connection",
                    },
                },
                "required": ["datasource", "statements"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "openWorldHint": true},
        }),
        json!({
            "name": "sqlx_results_rows",
            "title": "Read rows of a stored result",
            "description": "Read one page of a result that sqlx_sql_execute stored because it did not fit the printed preview. Pass the result `id` from that response; `statement` and `set` select the result set, which defaults to the first one. Read-only: it never contacts the database again.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Result UUID from the execute response"},
                    "statement": {"type": "integer", "minimum": 0, "description": "Statement index inside the result, counted from zero"},
                    "set": {"type": "integer", "minimum": 0, "description": "Result set index of that statement, counted from zero"},
                    "offset": {"type": "integer", "minimum": 0, "description": "First row to return"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "description": "Rows to return, 50 by default"},
                },
                "required": ["id"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "openWorldHint": false},
        }),
        json!({
            "name": "sqlx_prefetch",
            "title": "Prefetch SQLX components",
            "description": format!(
                "Download the components that would otherwise be fetched during a first query or page: {}. A first download can take minutes, so wait for the call instead of treating it as hung.",
                prefetch::CHOICES.join(", ")
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "components": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": prefetch::CHOICES,
                        },
                        "minItems": 1,
                    }
                },
                "required": ["components"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "openWorldHint": true},
        }),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }
    #[test]
    fn tools_declare_authorization_semantics() {
        let tools = tools();
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(
            names,
            [
                "sqlx_datasource_list",
                "sqlx_datasource_show",
                "sqlx_datasource_test",
                "sqlx_datasource_setup",
                "sqlx_datasource_setup_status",
                "sqlx_sql_execute",
                "sqlx_sql_view",
                "sqlx_results_rows",
                "sqlx_prefetch",
            ]
        );
        for tool in &tools {
            let annotations = &tool["annotations"];
            assert!(annotations["readOnlyHint"].is_boolean(), "{tool}");
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
        let execute = tools
            .iter()
            .find(|tool| tool["name"] == "sqlx_sql_execute")
            .expect("sqlx_sql_execute is declared");
        assert_eq!(execute["annotations"]["readOnlyHint"], false);
        assert_eq!(execute["annotations"]["destructiveHint"], true);
        assert!(
            execute["description"]
                .as_str()
                .unwrap()
                .contains("authorized"),
            "write tools must state the authorization requirement"
        );
    }
    #[test]
    fn prefetch_tool_lists_every_component() {
        let tools = tools();
        let prefetch_tool = tools
            .iter()
            .find(|tool| tool["name"] == "sqlx_prefetch")
            .expect("sqlx_prefetch tool is declared");
        let enum_values: Vec<&str> = prefetch_tool["inputSchema"]["properties"]["components"]
            ["items"]["enum"]
            .as_array()
            .expect("components enum is an array")
            .iter()
            .map(|value| value.as_str().expect("component names are strings"))
            .collect();
        assert_eq!(enum_values, prefetch::CHOICES.to_vec());
        let description = prefetch_tool["description"]
            .as_str()
            .expect("description is a string");
        for component in prefetch::CHOICES {
            assert!(
                description.contains(component),
                "description must mention {component}"
            );
        }
    }
    #[test]
    fn native_integrations_expose_every_mcp_tool() {
        // Codex and Claude Code call this server, while the dsh and pi packages register the same
        // tools natively and document them in their own README. Every list must stay identical.
        let mut server: Vec<String> = tools()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .map(str::to_owned)
            .collect();
        server.sort();
        assert_eq!(server.len(), 9);
        for (label, source) in [
            (
                "dsh tools",
                include_str!("../../../integrations/dsh/lib/index.js"),
            ),
            (
                "pi tools",
                include_str!("../../../integrations/pi/extensions/sqlx.ts"),
            ),
        ] {
            let mut registered: Vec<String> = source
                .lines()
                .filter_map(|line| line.trim().strip_prefix("name: \""))
                .filter_map(|rest| rest.strip_suffix("\","))
                .filter(|name| name.starts_with("sqlx_"))
                .map(str::to_owned)
                .collect();
            registered.sort();
            registered.dedup();
            assert_eq!(
                registered, server,
                "the {label} list must match the MCP server"
            );
        }
        for (label, source) in [
            (
                "dsh README",
                include_str!("../../../integrations/dsh/README.md"),
            ),
            (
                "pi README",
                include_str!("../../../integrations/pi/README.md"),
            ),
        ] {
            let mut documented: Vec<String> = source
                .lines()
                .filter_map(|line| line.trim().strip_prefix("| `"))
                .filter_map(|rest| rest.split('`').next())
                .filter(|name| name.starts_with("sqlx_"))
                .map(str::to_owned)
                .collect();
            documented.sort();
            documented.dedup();
            assert_eq!(
                documented, server,
                "the {label} tool table must match the MCP server"
            );
        }
    }
    #[test]
    fn native_integrations_install_the_cli_on_demand() {
        // The dsh and pi packages must be usable straight after installation: they resolve the CLI
        // themselves and install it once when it is missing.
        for (package, source, target) in [
            (
                "sqlx-dsh",
                include_str!("../../../integrations/dsh/lib/index.js"),
                "\"--target\", \"dsh\"",
            ),
            (
                "sqlx-pi",
                include_str!("../../../integrations/pi/extensions/sqlx.ts"),
                "\"--target\", \"pi\"",
            ),
        ] {
            assert!(
                source.contains("@ottermind/sqlx@latest"),
                "the {package} package must install the CLI when it is missing"
            );
            assert!(
                source.contains(target),
                "the {package} package must install the {package} Skill target"
            );
            assert!(
                source.contains("SQLX_INSTALL_DIR"),
                "the {package} package must honour SQLX_INSTALL_DIR"
            );
        }
        for (plugin, source, target) in [
            (
                "codex plugin",
                include_str!("../../../integrations/codex/plugins/sqlx/bin/sqlx-mcp"),
                "agent_target=codex",
            ),
            (
                "claude plugin",
                include_str!("../../../integrations/claude/plugins/sqlx/bin/sqlx-mcp"),
                "agent_target=claude",
            ),
        ] {
            assert!(
                source.contains("@ottermind/sqlx@latest"),
                "the {plugin} launcher must install the CLI when it is missing"
            );
            assert!(
                source.contains(target),
                "the {plugin} launcher must install its own Skill target"
            );
            assert!(
                source.contains("SQLX_INSTALL_DIR"),
                "the {plugin} launcher must honour SQLX_INSTALL_DIR"
            );
            assert!(
                source.contains("SQLX_VERSION=") && source.contains("runtime.json"),
                "the {plugin} launcher must install the version required by its manifest"
            );
        }
        // Codex passes only the variables named here to the MCP server, so the launcher's fallback
        // paths need HOME and LOCALAPPDATA to be on the list.
        let codex = include_str!("../../../integrations/codex/plugins/sqlx/.mcp.json");
        for variable in ["SQLX_INSTALL_DIR", "HOME", "LOCALAPPDATA", "PATH"] {
            assert!(
                codex.contains(variable),
                "the codex plugin must pass {variable} to the launcher"
            );
        }
    }
    #[test]
    fn initialize_negotiates_the_client_protocol() {
        let root = PathBuf::from("/tmp/sqlx-mcp-test");
        let mut sink = Vec::new();
        let response = handle(
            &request(1, "initialize", json!({"protocolVersion": "2024-11-05"})),
            &root,
            "unused",
            &None,
            &mut sink,
            None,
        )
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(response["result"]["serverInfo"]["name"], "sqlx");
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert!(response["result"]["instructions"]
            .as_str()
            .is_some_and(|instructions| instructions.contains("sqlx_datasource_list")));
    }
    #[test]
    fn notifications_and_unknown_methods_are_handled() {
        let root = PathBuf::from("/tmp/sqlx-mcp-test");
        let mut sink = Vec::new();
        assert!(handle(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &root,
            "unused",
            &None,
            &mut sink,
            None
        )
        .is_none());
        let response = handle(
            &request(2, "resources/list", json!({})),
            &root,
            "unused",
            &None,
            &mut sink,
            None,
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32601);
    }
    #[test]
    fn tool_failures_are_results_not_protocol_errors() {
        let root = PathBuf::from("/tmp/sqlx-mcp-test");
        let mut sink = Vec::new();
        let response = handle(
            &request(
                3,
                "tools/call",
                json!({"name": "sqlx_datasource_show", "arguments": {"id": "missing"}}),
            ),
            &root,
            "unused",
            &None,
            &mut sink,
            None,
        )
        .unwrap();
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"]["content"][0]["text"].is_string());
    }
    #[test]
    fn a_cancellation_notification_reaches_the_running_call() {
        let in_flight = InFlight::default();
        let running = CancellationToken::new();
        in_flight.start(&json!(7), running.clone());
        // A cancellation for a different request leaves this call alone.
        in_flight.cancel(&json!(8));
        assert!(!running.is_cancelled());
        in_flight.cancel(&json!(7));
        assert!(running.is_cancelled());
        // Once the call finished, a late cancellation belongs to nobody.
        in_flight.finish(&json!(7));
        let replacement = CancellationToken::new();
        in_flight.start(&json!(9), replacement.clone());
        in_flight.cancel(&json!(7));
        assert!(!replacement.is_cancelled());
    }
    #[test]
    fn two_calls_stay_cancellable_on_their_own() {
        let in_flight = InFlight::default();
        let first = CancellationToken::new();
        let second = CancellationToken::new();
        in_flight.start(&json!(1), first.clone());
        in_flight.start(&json!(2), second.clone());
        in_flight.cancel(&json!(1));
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        // A cancellation that arrives after the call was answered belongs to nobody.
        in_flight.finish(&json!(1));
        in_flight.cancel(&json!(1));
        assert!(!second.is_cancelled());
        in_flight.cancel(&json!(2));
        assert!(second.is_cancelled());
    }
    #[test]
    fn progress_is_reported_only_when_the_client_asked_for_it() {
        let mut sink = Vec::new();
        let mut progress =
            Progress::from_params(&json!({"_meta": {"progressToken": "p1"}}), &mut sink);
        progress.report(1, Some(3), "running statement 2 of 3");
        let notification = String::from_utf8(sink).unwrap();
        assert!(
            notification.contains("notifications/progress"),
            "{notification}"
        );
        assert!(
            notification.contains(r#""progressToken":"p1""#),
            "{notification}"
        );
        assert!(notification.contains(r#""total":3"#), "{notification}");
        assert!(notification.ends_with('\n'));
        let mut silent = Vec::new();
        let mut progress = Progress::from_params(&json!({}), &mut silent);
        progress.report(1, Some(3), "running statement 2 of 3");
        assert!(silent.is_empty());
    }
}
