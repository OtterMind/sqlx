# plugins branch

Distribution-only branch, generated from `integrations/` on `main` by
`.github/workflows/plugins-branch.yml`. Do not edit it by hand: the next sync overwrites it.

It carries the agent-harness marketplace manifests and the plugin shells so Codex and Claude Code
users install SQLX **without cloning the source tree**:

```sh
# Codex
codex plugin marketplace add OtterMind/sqlx@plugins
codex plugin add sqlx@ottermind

# Claude Code
claude plugin marketplace add OtterMind/sqlx@plugins     # or the /plugin equivalent
claude plugin install sqlx@ottermind
```

| Path | Purpose |
|---|---|
| `.agents/plugins/marketplace.json` | Codex marketplace (its native manifest) |
| `.claude-plugin/marketplace.json` | Claude Code marketplace |
| `plugins/sqlx/` | Codex plugin (`.codex-plugin/plugin.json`, `.mcp.json`, `bin/sqlx-mcp`) |
| `plugins/sqlx-claude/` | Claude Code plugin (`.claude-plugin/plugin.json`, `.mcp.json`, `bin/sqlx-mcp`) |

Both plugins launch `sqlx mcp`; each launcher reads the minimum CLI version from its plugin runtime
metadata, accepts only a compatible `SQLX_BIN`/PATH executable, and pins the official installer to
that version when the CLI is missing or too old. The CLI version in each plugin manifest matches
the repository release that generated the branch.
