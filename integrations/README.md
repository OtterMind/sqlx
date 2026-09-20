# SQLX agent-harness integrations

Native integrations that let an agent harness call SQLX as a tool instead of shelling out to the
CLI. All four share one tool surface: `sqlx mcp` (stdio MCP server) for MCP-capable harnesses, and
thin native-tool packages for harnesses that register tools themselves.

| Harness | Directory | Tool transport | Install |
|---|---|---|---|
| Claude Code | `claude/` | plugin `.mcp.json` → `sqlx mcp` | `claude plugin marketplace add <repo>` then `claude plugin install sqlx@ottermind` (or `--plugin-dir` while developing) |
| Codex | `codex/` | plugin `.mcp.json` → `sqlx mcp` | `codex plugin marketplace add <repo>` then `codex plugin add sqlx@ottermind` |
| DeepSeek Harness | `dsh/` | native `defineTool` tools | `dsh plugin --profile <profile> add @ottermind/dsh-sqlx` |
| Pi | `pi/` | native `registerTool` extension | `pi install npm:@ottermind/pi-sqlx` |

## Requirements

The `sqlx` CLI (0.1.7 or later, which provides `sqlx mcp`) must be installed and on `PATH`.
`SQLX_BIN` selects a specific executable; otherwise the first `sqlx` on `PATH` is used.

## Tools

| Tool | Purpose | Authorization |
|---|---|---|
| `sqlx_datasource_list` | List saved datasources (no secrets) | read-only |
| `sqlx_datasource_show` | Show one datasource | read-only |
| `sqlx_datasource_test` | Test connectivity | read-only |
| `sqlx_sql_execute` | Execute statements, return the full result | may modify data; ask the user first |
| `sqlx_sql_view` | Execute once and return a local result-page URL | may modify data; ask the user first |
| `sqlx_prefetch` | Download workers/JDBC/UI components | downloads only |

Write-capable tools are marked `destructiveHint` in MCP and say so in their descriptions, so a
harness that gates destructive tools (Codex does by default) asks the user before running them.

## Credentials

Store credentials in the SQLX data directory (`sqlx datasource add --connection-stdin`, or the
local `--ui` page). Datasources created with `--username-env`/`--password-env` read environment
variables at execution time, and a GUI-launched harness usually cannot see the user's shell
environment; MCP `env_vars` allowlists (Codex) make that explicit.

## Verified

- Codex CLI 0.155.1: plugin installs from a local marketplace, read-only MCP tools run headlessly,
  write tools require approval, `sqlx_sql_execute` returned `9007199254740993` exactly.
- DeepSeek Harness 0.1.5-rc.1: plugin installs into a scratch profile (`dsh plugin add <tarball>`),
  registers five native tools, and a headless run returned the same exact value.
- Claude Code 2.1.236: plugin and marketplace validate (`claude plugin validate --strict`); the MCP
  server was verified directly over stdio. A full `claude -p` run needs an authenticated session.
- Pi 0.86.0: package installs (`pi install`), and the extension loads through Pi's own jiti loader
  registering all five tools with valid schemas. A full `pi -p` run needs provider credentials.
