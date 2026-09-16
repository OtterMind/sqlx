SQLX 0.1.4 improves local page access, query result refresh recovery, and MySQL connection diagnostics.

## Improvements

- Reopen local pages without a five-minute launch deadline. The local UI stays running until explicitly stopped and keeps the same address after restart; reload an existing page to reconnect. Saved query results are still retained for 24 hours, and unfinished connection forms expire after 30 minutes.
- Use a more compact result view, with refresh and cancellation controls above the table and pagination below it. Single result sets no longer show an unnecessary tab, and automatic refresh displays its selected interval.
- MySQL connection errors identify the database address and port, distinguish refused connections from 15-second timeouts, and provide troubleshooting guidance when a connection is refused.

## Fixes

- Clear stale refresh errors after a successful refresh. When another page refreshes the same result, returning to the original page synchronizes its result and status without executing SQL again. Failed refreshes continue to preserve the last successful result and disable automatic refresh.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. To use all 0.1.4 page improvements, let active work finish, run `sqlx ui stop`, install `ui-default-0.1.4.zip` from this release with its SHA-256 from `SHA256SUMS`, select it with `sqlx ui plugin use default --version 0.1.4`, and run `sqlx ui` again. The new default plugin requires SQLX 0.1.4. Update managed Skills separately with `sqlx skill update` after upgrading the CLI.

Explicit result refresh still reruns the complete original SQL batch, including any writes. Reloading, paging, reconnecting, and returning to a page only read cached results.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.3...v0.1.4
