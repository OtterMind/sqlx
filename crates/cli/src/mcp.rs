//! Minimal Model Context Protocol server over stdio, exposing SQLX tools to agent harnesses.
//!
//! The harness (Claude Code, Codex, or a thin native-tool shim) launches `sqlx mcp`; SQLX keeps
//! protocol output on stdout, so execution results are rendered into the tool response instead of
//! being streamed as CLI JSON. Downloads and worker diagnostics still go to stderr.
use crate::{
    execution, prefetch,
    storage::{Datasource, Store},
    ui,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sqlx_protocol::Action;
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

/// Protocol revision used when the client does not request one.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn serve(root: PathBuf, manifest: String, local: Option<PathBuf>) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                respond(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": {"code": -32700, "message": format!("parse error: {error}")},
                    }),
                )?;
                continue;
            }
        };
        if let Some(response) = handle(&request, &root, &manifest, &local) {
            respond(&mut stdout, &response)?;
        }
    }
    Ok(())
}
fn respond(output: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
/// Handle one JSON-RPC message. Notifications (no `id`) produce no response.
fn handle(request: &Value, root: &Path, manifest: &str, local: &Option<PathBuf>) -> Option<Value> {
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
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => call(params, root, manifest, local),
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
fn call(params: Value, root: &Path, manifest: &str, local: &Option<PathBuf>) -> Result<Value> {
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
            run_action(root, manifest, local, source, Action::Test, vec![])?
        }
        "sqlx_sql_execute" => {
            let source = datasource_arg(root, &arguments)?;
            let statements = statements_arg(&arguments)?;
            run_action(root, manifest, local, source, Action::Execute, statements)?
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
            let (report, complete) = prefetch::run(root, manifest, &components)?;
            json!({"complete": complete, "report": report})
        }
        other => bail!("unknown tool {other}"),
    };
    Ok(json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&value)?}],
        "isError": false,
    }))
}
fn run_action(
    root: &Path,
    manifest: &str,
    local: &Option<PathBuf>,
    source: Datasource,
    action: Action,
    statements: Vec<String>,
) -> Result<Value> {
    let mut buffer = Vec::new();
    let success = execution::run_to(
        &mut buffer,
        root.to_path_buf(),
        manifest.to_owned(),
        local.clone(),
        source,
        action,
        statements,
    )?;
    let events: Value =
        serde_json::from_slice(&buffer).context("execution returned invalid JSON")?;
    Ok(json!({"success": success, "execution": events}))
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
            "name": "sqlx_sql_execute",
            "title": "Execute SQL with SQLX",
            "description": "Execute one or more complete SQL statements through SQLX and return the full structured result. Each statement is a separate driver statement and the batch stops at the first error. Statements may modify data: only call this after the user authorized that exact operation and scope. Results are never replayed automatically.",
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
            "name": "sqlx_prefetch",
            "title": "Prefetch SQLX components",
            "description": format!(
                "Download the components that would otherwise be fetched during a first query or page: {}. Progress and speed are reported through the MCP client's server log.",
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
                "sqlx_sql_execute",
                "sqlx_sql_view",
                "sqlx_prefetch",
            ]
        );
        for tool in &tools {
            let annotations = &tool["annotations"];
            assert!(annotations["readOnlyHint"].is_boolean(), "{tool}");
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
        let execute = &tools[3];
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
        assert_eq!(server.len(), 6);
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
    }
    #[test]
    fn initialize_negotiates_the_client_protocol() {
        let root = PathBuf::from("/tmp/sqlx-mcp-test");
        let response = handle(
            &request(1, "initialize", json!({"protocolVersion": "2024-11-05"})),
            &root,
            "unused",
            &None,
        )
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(response["result"]["serverInfo"]["name"], "sqlx");
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
    }
    #[test]
    fn notifications_and_unknown_methods_are_handled() {
        let root = PathBuf::from("/tmp/sqlx-mcp-test");
        assert!(handle(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &root,
            "unused",
            &None
        )
        .is_none());
        let response = handle(
            &request(2, "resources/list", json!({})),
            &root,
            "unused",
            &None,
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32601);
    }
    #[test]
    fn tool_failures_are_results_not_protocol_errors() {
        let root = PathBuf::from("/tmp/sqlx-mcp-test");
        let response = handle(
            &request(
                3,
                "tools/call",
                json!({"name": "sqlx_datasource_show", "arguments": {"id": "missing"}}),
            ),
            &root,
            "unused",
            &None,
        )
        .unwrap();
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"]["content"][0]["text"].is_string());
    }
}
