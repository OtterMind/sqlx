SQLX 0.1.15 opens SQLite, DuckDB and H2 files, and pins the CLI version a Codex or Claude Code plugin needs.

## New

- `--type sqlite` (alias `sqlite3`) and `--type duckdb` open a local database file with `--path <file>` or `--database <file>`, and create it when it is missing. They take no host, port or credentials and run in their own native workers, so neither needs a JVM or a server. `--property mode=ro` and `--property busy_timeout=<ms>` tune SQLite, `--property read_only=true` and `--property threads=<n>` tune DuckDB, and `:memory:` keeps a database for one invocation.
- `--type h2` uses the JDBC worker for the same file mode, or reaches an H2 TCP server with `--host` and `--port` (9092 by default). Its driver ships as a new release component, and an embedded database defaults to the `sa` user.
- Values follow each engine. SQLite reports the declared type, or the kind of the first value, and encodes blobs as Base64. DuckDB keeps `BIGINT`, `HUGEINT` and `DECIMAL` exact, renders timestamps as ISO text, nested values as JSON text and blobs as Base64, and answers a write with a `Count` result set.
- A `--command` that carries more than one statement is rejected on DuckDB and H2, because their drivers would run the rest without reporting it; SQLite refuses such an argument itself.
- `sqlx prefetch` accepts `sqlite`, `duckdb` and `h2`, the connection page lists the three types and asks for a file instead of a host, port and credentials, and the Skill carries one recipe per engine — twenty-three in total — and requires CLI 0.1.15.
- The Codex and Claude Code plugins carry a `runtime.json`: the launcher installs the CLI version the plugin needs and refuses to start an older, incompatible MCP server instead of failing later. The `plugins` branch is generated from it.
- The default interface moves to 0.1.6 with the new engine types; starting the UI installs and selects it as usual.
- A new CI job exercises SQLite, DuckDB and embedded H2 without any container.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing, so adding the plugin is the only setup step.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin, and downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.15 Skill requires SQLX 0.1.15.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied command unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.14...v0.1.15
