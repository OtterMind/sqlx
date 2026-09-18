SQLX 0.1.6 adds a command that downloads database workers and the local UI ahead of time, and reports download progress, speed and elapsed time.

## New

- `sqlx prefetch <component>` downloads what would otherwise be fetched during the first query or page: `mysql`, `postgres`, `oracle`, `sqlserver`, `ui`, `skill` or `all`. Each component reports `downloaded`, `already_installed`, `unavailable` or `failed`; `oracle` and `sqlserver` include the shared JDBC runner and JRE, and `all` covers every component. The command exits non-zero when a component fails and can be re-run to continue with the components that are already installed.

## Improvements

- Every download (database worker, JDBC runtime, UI engine, UI plugin, Skill and release manifest) prints progress with speed and estimated time on stderr, followed by `Downloaded … in 12.3s (390 KB/s)`. An interrupted transfer is retried up to three times instead of failing immediately, and a failed command explains that it can be re-run.
- A slow transfer is no longer killed after five minutes. The transfer budget is 30 minutes, so a component can finish on a slow connection; the progress line shows that it is still moving. Progress is refreshed only when stderr is a terminal, so piped JSON output is unchanged.
- The shipped Skill now states which resources download on first use, roughly how large they are, and how to prefetch them when the network is slow.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.6 Skill requires SQLX 0.1.6.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.5...v0.1.6
