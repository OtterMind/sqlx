SQLX 0.1.12 adds Redis and MongoDB, and makes `--command` the flag that carries a statement.

## New

- `--type redis` connects to Redis on port 6379 and runs in its own native Rust worker. Each `--command` is one Redis command, such as `GET session:1` or `HGETALL user:7`; replies become rows, pairs keep a `field` and `value` column, nested arrays use one column per position, and binary values use Base64 with their type.
- `--type mongodb` (alias `mongo`) connects to MongoDB on port 27017 in its own native Rust worker. Each `--command` is one command document, such as `{"find":"users","filter":{"active":true}}`; `insertOne`, `findOne` and the other helpers are accepted as an alternative spelling, cursors are read to the end, `count` and `distinct` return their own column, and nested values use canonical extended JSON.
- `--command` now carries one statement for every database and `--sql` remains an alias, so existing scripts keep working. MCP tool names, the `statements` parameter, saved datasources and the page API are unchanged.
- Neither engine needs the JDBC worker or a JRE: `sqlx prefetch` accepts `redis` and `mongodb`, and connecting downloads only that worker. The Skill recipes `references/redis.md` and `references/mongodb.md` document the reply mapping of both engines.
- The Skill carries one reference file per engine — twenty in total — and requires CLI 0.1.12. The connection form and the saved-datasource view list all twenty types.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing, so adding the plugin is the only setup step.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. The default interface moves to 0.1.5 with this release and now follows the CLI: starting the UI installs the interface named by the release manifest and selects it when the installed interface is the default or nothing is selected, so an updated CLI also serves its own connection form. An interface you installed yourself keeps the version you selected. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.12 Skill requires SQLX 0.1.12.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied command unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.11...v0.1.12
