# SQLX v0.1.3 release acceptance

Date: 2026-09-11. [v0.1.3](https://github.com/OtterMind/sqlx/releases/tag/v0.1.3) is published as the latest stable release. The installed user CLI was upgraded from the public 0.1.2 executable using its own update command.

## Source and artifacts

- Annotated tag source: `6b37b23c4bf09b01947b1b9731f55385cfa93857`. The annotation records the source, workflow commit/ref, all five platforms and `publish=true`.
- [Source CI](https://github.com/OtterMind/sqlx/actions/runs/34583811849): all nine jobs passed, including five platform targets and MySQL/PostgreSQL/Oracle/SQL Server integration.
- [Packaging and publication](https://github.com/OtterMind/sqlx/actions/runs/34584508381): all seven jobs passed, including Developer ID signing and Apple notarization for both macOS architectures.
- All 28 public files were independently downloaded. All 27 SHA256SUMS entries, GitHub asset digests and manifest package hashes matched. All 25 ZIPs passed integrity checks.
- All 20 native executables matched their declared architecture. All eight macOS executables passed independent Developer ID signature verification.
- The manifest contains 30 components, including five pinned upstream JRE downloads. GitHub Latest and the public version pointer identify 0.1.3.

## Real 0.1.2 to 0.1.3 self-update

The user's existing 0.1.1 installation was first upgraded to public 0.1.2 with the official installer. Before 0.1.3 was published, `update check`, equal-version `update install` and `update status` were exercised against the official 0.1.2 release. A separate data profile held a real PostgreSQL connection, a managed 0.1.2 Skill and a running public 0.1.2 UI service.

After publication, the **installed user executable** performed the following checks using the official update source and normal installation target. No installer script or local release fixture replaced the self-update operation.

| Scenario | Verified result |
|---|---|
| `update check` from 0.1.2 | Reported `update_available` and latest version 0.1.3 |
| `update install` | Returned `installed`, from 0.1.2 to 0.1.3; the executable at the installed path reported 0.1.3 |
| `update status` | Reported current version 0.1.3 and the successful installation receipt |
| Subsequent check | Reported `up_to_date` for 0.1.3 |
| In-flight SQL | A 45-second PostgreSQL query started under 0.1.2 remained active after executable replacement and completed successfully with the exact value `9007199254740993` |
| Existing UI | Its PID/instance and selected 0.1.2 plugin remained unchanged through executable replacement |
| Encrypted storage | User and fixture master keys, identity files and encrypted datasource files remained byte-for-byte unchanged |
| Existing connections | The preserved PostgreSQL fixture and the user's existing loopback MySQL connection passed through publicly downloaded 0.1.3 workers |
| Managed Skills | Both the test Skill and the user's managed Skill updated to 0.1.3; status reported the user installation intact |
| Explicit version | Equal-version installation was a no-op; requesting 0.1.2 from 0.1.3 was rejected without changing the binary. These negative/no-op checks used a separate update-history directory so the user's successful installation receipt was retained |

Source CI additionally covers wrong executable identity, download corruption, unsafe archives, candidate timeout, failed verification at the installed path, original-binary restoration and concurrent update writers on all five targets.

## Public UI and fresh installation

UI upgrade was a separate, explicit step after proving that self-update preserved the running service and plugin selection. The test service was stopped, the public 0.1.3 default plugin installed by its release URL/SHA-256, and the public 0.1.3 service downloaded and started. The user's stopped default UI installation was also updated, and the existing preview selected the public plugin without restarting its service.

- The result saved by the 0.1.2 UI remained available under its original result ID. The 0.1.3 workbench displayed the saved datasource and retained exact values.
- The combined Refresh control reran the saved query and replaced its snapshot at the same URL. Its menu enabled a five-second interval and returned to manual mode correctly. Dark mode survived reload.
- Restarting the public 0.1.3 service preserved the refreshed snapshot and its metadata exactly; reading it after restart did not execute SQL again.
- A fresh installation using the public Latest download resolved to 0.1.3. Version/help/init and managed Skill installation passed. Initialization created only key, identity, encrypted storage and lock files, without downloading database workers or a JRE.

## Source browser and API regression coverage

Playwright CLI also validated the full changed interface against an isolated PostgreSQL fixture before publication:

- Saved datasource listing, details, connection testing, editing with the saved password, cancellation without saving, immediate sidebar rename, removed-source feedback and empty states.
- Mouse and keyboard menu selection, Escape/outside-click dismissal, interval display on the combined button and equal alignment of its two click areas.
- Manual and timed refresh of an unchanged two-statement SQL batch containing an UPDATE followed by a SELECT. Counter values proved execution order, prevention of duplicate requests, and that disabling the interval stopped subsequent execution.
- Old rows remained visible during refresh. A database failure preserved those rows and disabled automatic refresh without retrying.
- Delaying the main application script left the first styled page dark, with the saved theme already applied by the small head bootstrap script. The dark-mode fix did not depend on the main application or API response arriving quickly.

API/unit tests cover sliding server/cookie expiry without reviving expired sessions, authentication/Origin checks, multiple result sets, stale snapshot reads, durable refresh request IDs, rejection of overlapping refreshes, failure/cancellation preservation, refresh preparation failure, completed snapshot recovery, interrupted refresh recovery, and retention of prior outcomes after later refreshes. The refresh endpoint executes the stored SQL exactly; it does not classify or rewrite it. Preserving cached results does not roll back database writes.

## Scope and cleanup

Public self-update and browser acceptance ran on macOS ARM64. Other platforms were covered by source CI and public artifact verification; the complete public-download flow was not repeated interactively on every platform. Oracle and SQL Server were exercised in CI, while local public-release connectivity covered MySQL and PostgreSQL.

Acceptance-owned SQL, UI services, headless browsers and temporary installation/download directories were cleaned up. The user's CLI, managed Skill and default UI plugin remain at 0.1.3, with the real installation receipt retained. The user's saved connections, existing preview and its supporting databases were preserved.
