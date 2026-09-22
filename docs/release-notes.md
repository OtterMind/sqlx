SQLX 0.1.13 prints a preview and stores the full result, so a large answer no longer has to enter the conversation.

## New

- Every execution prints one table-shaped JSON object: `results[].stmt/cols/rows/count/affected`, a per-statement `error`, the indexes of statements that never ran in `skipped`, and a final `success` that matches the exit status. A two-row answer needs 158 bytes instead of 744; a 100-row answer 364 instead of 8479.
- A result set larger than the preview — 10 rows and 16 KiB by default — is written completely to `<result-dir>/<id>/<statement>-<result>.jsonl`, and its item carries that `file` instead of the remaining rows. Read the file directly or page it with `sqlx results rows --id <id> --offset 100 --limit 50`; neither touches the database again.
- `sqlx setting list|get|set|unset` configures the preview size, the result directory and the retention. The directory defaults to a private per-user directory inside the system temporary directory, so results disappear when the machine reboots; point it at a persistent path to keep them. Results are removed after 24 hours and stay under 1 GiB in total, and `SQLX_PREVIEW_ROWS`, `SQLX_RESULTS_DIR` and `SQLX_RESULTS_RETENTION_HOURS` override the file.
- `--events` prints the raw worker event stream (`protocol_version`, `datasource_id`, `events`, `success`) for scripts that parse it, and `--preview <rows>` overrides the preview size for one call; `--preview 0` prints no rows at all.
- MCP: `sqlx_sql_execute` returns the table shape directly instead of wrapping it in `execution`, and the new read-only `sqlx_results_rows` reads a stored result for harnesses without file access. `sqlx_datasource_test` answers `{"success":true}` instead of a handshake log.
- Fixed: a worker that could not start, a database that refused the connection, or a stream that broke mid-result used to print a truncated JSON object that no parser could read. Every path now prints exactly one valid object, with the outcome (`failed`, `unknown`) preserved.
- The Skill follows the new shape, adds `references/results.md` for stored results and settings, and requires CLI 0.1.13.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing, so adding the plugin is the only setup step.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

This release changes the printed result: scripts that parse the old `events` array must pass `--events`, which prints it unchanged and stores nothing. CLI updates preserve running UI services and the selected plugin, and downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.13 Skill requires SQLX 0.1.13.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied command unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.12...v0.1.13
