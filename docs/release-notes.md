SQLX 0.1.11 adds TiDB, StarRocks, Apache Doris and YugabyteDB.

## New

- `--type tidb` connects to TiDB on port 4000, and `--type starrocks` and `--type doris` connect to StarRocks and Apache Doris on port 9030. All three speak the MySQL protocol and reuse the MySQL worker.
- `--type yugabytedb` (aliases `yugabyte` and `yb`) connects to YugabyteDB on port 5433 and reuses the PostgreSQL worker.
- No new component is downloaded: every new engine resolves to the existing `mysql` or `postgres` worker, so their drivers, licenses and pinned versions do not change. `sqlx prefetch` accepts all four new names.
- The Skill carries one reference file per engine — twelve in total — and requires CLI 0.1.11. The local connection page and the saved-datasource view list all twelve types.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing, so adding the plugin is the only setup step.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.11 Skill requires SQLX 0.1.11.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.10...v0.1.11
