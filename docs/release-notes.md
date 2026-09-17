SQLX 0.1.5 adds an npx install path and makes the shipped Skill confirm state-changing SQL before it runs.

## New

- Install or update the CLI and its Skill with one command: `npx -y @ottermind/sqlx@latest`. The installer verifies the release manifest, `SHA256SUMS` and the downloaded archive before it replaces the CLI, then installs the Skill into `./sqlx`. Use `--target codex`, `--target claude` or `--target <directory>` to put the Skill somewhere else. Node.js is required only for the installation: the CLI still has no Node.js runtime dependency, and the Shell and PowerShell installers remain available for machines without Node.js. The npm package `@ottermind/sqlx` is published at 0.1.5, the same version as this release.

## Improvements

- The shipped Skill classifies every batch as read-only, state-changing or unknown before executing it, states the target, the statements, the expected change and the scope, and asks for confirmation when that operation has not been authorized. The rule also covers `--view`, the page's **Refresh** action and the refresh interval, so a one-time approval does not authorize repeated writes.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.5 Skill requires SQLX 0.1.5.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.4...v0.1.5
