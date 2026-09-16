# SQLX v0.1.4 release acceptance

Date: 2026-09-16. [v0.1.4](https://github.com/OtterMind/sqlx/releases/tag/v0.1.4) is published as the latest stable release with English release notes.

## Source and build verification

- Annotated tag source: `afb5b8f404ae7c84203729330128a5c9da7dea47`. The annotation records the source repository/commit, workflow ref/commit, five platforms, stable channel, and `publish=true`.
- [Source CI](https://github.com/OtterMind/sqlx/actions/runs/35078572983): all nine jobs passed on the tagged commit, including five platform targets and MySQL/PostgreSQL/Oracle/SQL Server integration.
- [Packaging and publication](https://github.com/OtterMind/sqlx/actions/runs/35078588149): all seven jobs passed, including Developer ID signing and Apple notarization for both macOS architectures.
- Local frontend build, Rust formatting, Clippy with warnings denied, workspace build and all 19 Rust tests passed. Maven verification passed both JDBC unit tests.
- Local distribution, installer, UI lifecycle/distribution/plugin and self-update regression scripts passed. The named local Rust toolchain was incomplete; the installed `stable` toolchain reported Rust/Cargo 1.95.0 and was selected explicitly for these commands.
- Playwright CLI exercised all eight `tests/ui_refresh_recovery.py` scenarios against a dedicated MySQL fixture: initial display, outage, historical error display, manual recovery, a second outage, recovery on focus, automatic refresh recovery and service restart. Cached rows and exact integers survived; failure stopped automatic refresh; page reload/focus/restart did not replay SQL. This browser script was run locally and is not part of CI.

## Public artifacts

- All 28 release files were downloaded independently. All GitHub asset digests and 27 SHA256SUMS entries matched. All 25 ZIP files passed integrity checks.
- All 30 manifest components were present, including five pinned upstream JRE entries. Every release-hosted component points to v0.1.4 and matches its archive checksum.
- All 20 native executables matched their platform/architecture. All eight macOS executables independently passed strict Developer ID signature verification.
- GitHub Latest and its public `release-version.txt` identify 0.1.4. The published English body matches `docs/release-notes.md`.
- The default UI plugin and Skill declare a minimum CLI version of 0.1.4. The public 0.1.3 executable correctly rejected installation of the new default plugin.

## Real public 0.1.3 to 0.1.4 self-update

The upgrade used a disposable installation of the public, checksum-verified 0.1.3 executable, an isolated data directory, a real MySQL connection, a managed 0.1.3 Skill, a retained query result and a running public 0.1.3 UI service. The executable used the normal public update source and replaced itself with 0.1.4.

| Scenario | Verified result |
|---|---|
| Update check/install/status | Reported an available update, installed 0.1.4 and retained a successful receipt from 0.1.3 to 0.1.4; a subsequent check reported up to date |
| In-flight SQL | A 45-second MySQL query remained active after executable replacement, completed successfully and preserved `9007199254740993` exactly |
| Existing UI and plugin | The running UI PID/instance and selected plugin were unchanged during CLI replacement |
| Encrypted data | Master key, identity and encrypted datasource files remained byte-for-byte unchanged |
| Managed Skill | Updated separately to 0.1.4 after the CLI upgrade |
| New UI | After stopping the old service and explicitly installing/selecting the public 0.1.4 plugin, the new service opened a plain local URL and established a browser session |
| Retained result | The result created under 0.1.3 remained readable with its original ID and exact integer value |
| MySQL worker | The upgraded installation connected successfully through the public 0.1.4 worker |
| Service restart | A second 0.1.4 service start reused its saved address; reopening the page retained the same completed result snapshot |

A separate fresh installation used the public v0.1.4 installer script and the public Latest download. Version/help/init and managed Skill installation passed. Initialization did not download database workers or a JRE.

## Scope

Public installation and self-update acceptance ran on macOS ARM64. Other platforms were covered by source CI, packaging, architecture checks and public artifact verification; public self-update was not repeated interactively on every platform. The local public-release database check used MySQL; PostgreSQL, Oracle and SQL Server were covered by tagged-source CI.

Acceptance used dedicated temporary installations, an isolated MySQL fixture and test-owned UI/browser sessions. The user's installed CLI, Skills, saved connections and existing services were not upgraded or restarted.

Test-owned UI/browser processes, the MySQL fixture, temporary downloads, disposable installations and test data were cleaned up after verification.
