# SQLX agent-harness integrations

Native integrations that let an agent harness call SQLX as a tool instead of shelling out to the
CLI. All four share one tool surface: `sqlx mcp` (stdio MCP server) for MCP-capable harnesses, and
thin native-tool packages for harnesses that register tools themselves.

| Harness | Directory | Tool transport | Install |
|---|---|---|---|
| Claude Code | `claude/` | plugin `.mcp.json` → `sqlx mcp` | `claude plugin marketplace add <repo>` then `claude plugin install sqlx@ottermind` (or `--plugin-dir` while developing) |
| Codex | `codex/` | plugin `.mcp.json` → `sqlx mcp` | `codex plugin marketplace add <repo>` then `codex plugin add sqlx@ottermind` |
| DeepSeek Harness | `dsh/` | native `defineTool` tools | `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` |
| Pi | `pi/` | native `registerTool` extension | `pi install npm:@ottermind/sqlx-pi` |

## Requirements

The `sqlx` CLI (0.1.9 or later, which provides `sqlx mcp`). Every integration resolves it the same
way: `SQLX_BIN`, then `sqlx` on `PATH`, then the official user-level installation
(`SQLX_INSTALL_DIR`, `~/.local/bin/sqlx`, `%LOCALAPPDATA%\Programs\SQLX\sqlx.exe`), and when none of
them exists it installs the CLI itself with the official npm installer, so installing the plugin or
the extension is the only setup step. The official location also covers harnesses started from a
GUI, where `~/.local/bin` is normally missing from `PATH`.

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

## Verified end-to-end (2026-09-20)

Each harness ran the same task headlessly: list datasources, then execute a read-only `SELECT`
against a PostgreSQL datasource and reply with the exact `big` value. All four returned
`9007199254740993` unchanged. Codex, Claude Code and Pi were re-verified after upgrading to
codex-cli 0.155.1, Claude Code 2.1.278 and Pi 0.86.1 respectively.

| Harness | Command that was verified |
|---|---|
| Codex CLI 0.155.1 (latest) | `codex plugin marketplace add <repo>` → `codex plugin add sqlx@ottermind` → `codex exec --skip-git-repo-check "<task>"` |
| DeepSeek Harness 0.1.5-rc.1 | `dsh plugin --profile sqlxtest add <tarball>` → `dsh --profile sqlxtest "<task>"` |
| Claude Code 2.1.278 | `claude --plugin-dir integrations/claude/plugins/sqlx --allowedTools "mcp__plugin_sqlx_sqlx__*" -p "<task>"` |
| Pi 0.86.1 | `pi -e integrations/pi/extensions/sqlx.ts -e <provider>.ts --provider deepseek --model deepseek-flash --no-session -p "<task>"` |

Notes from the verification:

- **Codex gates write tools.** Read-only tools ran without approval; `sqlx_sql_execute` was refused
  with `MCP tool call requires approval, but approval policy is never` until approvals were granted.
  That is the intended behaviour: keep the default and let the user approve.
- **Claude Code needs an explicit tool allowlist in headless mode**, e.g.
  `--allowedTools "mcp__plugin_sqlx_sqlx__*"`, otherwise the MCP call returns without permission.
- **Pi needs a model provider.** Any OpenAI-compatible provider works; registering DeepSeek looks
  like this in a second `-e` extension:
  `pi.registerProvider("deepseek", { baseUrl: "https://api.deepseek.com", apiKey: "$DEEPSEEK_API_KEY", api: "openai-completions", models: [{ id: "deepseek-flash", name: "DeepSeek Flash", reasoning: true, input: ["text"], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 262144, maxTokens: 8192 }] })`.
- **Claude Code can use DeepSeek's Anthropic-compatible endpoint**: `ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic`
  with `ANTHROPIC_AUTH_TOKEN` and `ANTHROPIC_MODEL=deepseek-flash[1m]`.
