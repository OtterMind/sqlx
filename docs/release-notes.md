SQLX 0.1.14 lets an agent take a large result inline when it cannot read the stored file.

## New

- `--full` on `sqlx sql execute` prints every row of every result set and stores nothing, so an agent without file access still receives the complete answer in the table shape.
- `sqlx setting set result-mode full` makes that the default for a machine, and `preview` (the default) keeps the bounded preview with the rest in the stored file. `sqlx setting list` reports the effective value and where it comes from, and `SQLX_RESULT_MODE` overrides the file for one environment.
- The MCP tool `sqlx_sql_execute` accepts `"full": true` for one call, and the DeepSeek Harness and Pi tool schemas accept the same argument and translate it to `--full`.
- Full mode ignores the preview row count and the 16 KiB value budget, and never writes a result file or an `id`/`file` field. The answer then has no size limit of its own, which is why the preview stays the default.

Everything else is unchanged from 0.1.13: the table-shaped result, `--events` for the raw worker stream, `sqlx results list|rows` for stored results, and `sqlx setting` for the preview size, the result directory and the retention.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing, so adding the plugin is the only setup step.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin, and downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.14 Skill requires SQLX 0.1.14.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied command unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.13...v0.1.14
