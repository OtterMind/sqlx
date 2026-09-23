SQLX 0.1.16 imports saved connections from another tool in one call, and keeps every credential off the command line.

## New

- `sqlx datasource import` reads a versioned JSON document from `--stdin` or `--file <path>` and stores every connection in one write. Entries merge by name: an unknown name is added, a known name is updated in place and keeps its datasource ID, and nothing is ever removed.
- Each entry is validated on its own, so an engine SQLX does not speak, or a connection missing the fields its engine needs, is reported in `skipped` with a reason instead of failing the whole document. Every entry carries the same connection object `--connection-stdin` accepts.
- `--dry-run` validates and reports without storing anything, and `--strict` refuses the entire document when any entry is invalid, so a scripted migration never half-applies. Catalogues arrive on standard input, or from a file for callers that cannot pipe.
- `examples/import-connections.json` is a runnable starting point — PostgreSQL, MySQL and a local SQLite file — and a contract test imports it on every build, so the document format cannot drift unnoticed.
- The Skill gained a bulk-import recipe in `references/connections.md`, for agents that are handed a connection list instead of entering connections one by one.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing, so adding the plugin is the only setup step.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin, saved datasources and stored results; downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update`. The 0.1.16 Skill still requires CLI 0.1.15 or newer, and only the bulk import needs 0.1.16.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied command unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.15...v0.1.16
