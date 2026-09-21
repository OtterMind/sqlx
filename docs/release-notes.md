SQLX 0.1.9 serves SQLX over the Model Context Protocol and ships ready-made integrations for four agent harnesses.

## New

- `sqlx mcp` runs SQLX as a Model Context Protocol server over stdio, exposing `sqlx_datasource_list`, `sqlx_datasource_show`, `sqlx_datasource_test`, `sqlx_sql_execute`, `sqlx_sql_view` and `sqlx_prefetch`. Read-only tools are annotated as read-only; both execution tools are marked destructive and state in their description that the user must authorize the operation and its scope first, so a harness that gates destructive tools asks before running them.
- Integrations for four harnesses: Codex and Claude Code install a plugin that starts the MCP server, while DeepSeek Harness and Pi install a package that registers the same tools natively. Each integration lives under `integrations/` and requires the SQLX CLI on `PATH`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.9 Skill requires SQLX 0.1.9.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.8...v0.1.9
