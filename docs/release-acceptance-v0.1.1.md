# SQLX v0.1.1 release acceptance

Date: 2026-09-11. Published release: [v0.1.1](https://github.com/OtterMind/sqlx/releases/tag/v0.1.1), marked Latest. Public-download acceptance passed on macOS ARM64.

## Source and release artifacts

- Annotated tag source: `4f2602c01f345fc1f60610a22c0482ebdc3e7b51` (PR #1 merged).
- [Source CI](https://github.com/OtterMind/sqlx/actions/runs/34554787284): all nine jobs passed, including five platform targets and MySQL/PostgreSQL/Oracle/SQL Server integration.
- [Package build](https://github.com/OtterMind/sqlx/actions/runs/34554854822): all five native package jobs and the shared package job passed. Both macOS targets completed Developer ID signing and Apple notarization; assembly validated 30 manifest entries.
- The initial automatic publication step failed because checkout's shallow SHA fetch produced a local lightweight tag, so the annotation check rejected it. The unchanged assembled artifacts were downloaded, checked against their SHA-256 manifest and macOS signatures, and published after verifying the source CI and remote annotated tag. All uploaded GitHub asset digests were verified before the draft became public. The workflow now explicitly fetches the annotated tag; this fix is recorded in `19ef8f69b81ea8524d07746927bd4a5f458c63c5` and passed CI.
- 28 public files: 25 independent ZIPs, `manifest.json`, `SHA256SUMS` and `release-version.txt`.
- All 27 checksum entries matched independently downloaded public files. All ZIPs passed integrity checks, all 20 native binaries matched their intended architectures, and all eight macOS binaries passed independent Developer ID signature verification.
- The manifest contains 30 components: 20 native executables, five shared packages and five pinned upstream JRE downloads. The UI service and default plugin download independently.

## Public installation and upgrade

The executable, workers, JRE, JDBC drivers, default UI and Skill came from public GitHub Release downloads. Acceptance did not use local workers, a custom manifest, a development Java override, or the CI bundle as an installation shortcut. Dedicated install/data/Skill paths isolated the user's existing SQLX installation.

| Scenario | Result |
|---|---|
| README installer | Installed public 0.1.0, then reran the public installer without a version override and received 0.1.1 |
| Upgrade preservation | Before any form edits, the master key, identity file and encrypted datasource file remained byte-for-byte unchanged; the saved PostgreSQL connection still worked |
| Old manifest cache | Upgrade fetched the current release manifest and automatically installed the UI service and default plugin despite the retained 0.1.0 cache |
| Managed Skill upgrade | Updated the existing Skill to the published 0.1.1 version, including browser/plugin instructions |
| Skill-first fresh install | Extracted the published Skill first, followed its CLI guide into a separate empty installation, initialized storage and installed a managed Skill |
| Initial footprint | Initialization did not download a database worker or JRE |
| Plugin distribution | Installed the default plugin using its public GitHub ZIP URL and SHA-256, plus a local build of the documented independent terminal UI example |

An initial v0.1.0 download through a local proxy stalled; the same public installer succeeded on a direct retry. The 0.1.1 downloads and upgrade used the public URLs successfully.

## Database and browser behavior

MySQL 8.4 and PostgreSQL 17 used isolated Docker fixtures. Oracle Free 23 and the existing Azure SQL Edge target were temporarily started for the authorized local tests; separate CI also covered SQL Server 2022.

- The published CLI/service saved browser-submitted credentials and executed two SQL statements for MySQL, Oracle and SQL Server, then displayed each result set with the exact value `9007199254740993`. Normal CLI execution also succeeded through the downloaded workers and private JRE.
- PostgreSQL was first saved by 0.1.0 and tested again after upgrading. Playwright CLI then opened the published default UI, replaced credentials, observed incorrect-password feedback, corrected the password, submitted with Enter, and verified the completed state after reload. Setup/status responses omitted passwords and test credentials were absent from plaintext state/log files.
- The published result UI displayed 251 PostgreSQL rows and a second result set. Browser checks covered pagination, result-set selection, exact integer values, NULL/empty strings, full-cell inspection/copy and refresh. HTML-like database values remained text; the browser console had no errors or warnings.
- Selecting the terminal UI changed the rendered interface while the same result ID remained available. Its result-set selection, pagination and reload worked through the published service.
- A PostgreSQL sequence stayed at 251 after pagination, UI switching, refresh and service restart, proving these reads did not replay the query. After restart, the final page still contained 51 rows.
- A cancelled setup did not write a connection. A batch stopped at its first SQL error and marked the subsequent statement skipped. Cancelling a PostgreSQL sleep query produced a cancelled result.

## Limits and cleanup

Interactive public-release testing ran on macOS ARM64. CI separately covered all five supported OS/architecture targets; the complete public-download/browser/database flow was not repeated interactively on every platform. The local database tests used username/password authentication and explicitly disabled TLS for the test targets.

The test sequence, disposable SQLX installations, browser session and dedicated MySQL/PostgreSQL containers were removed. Oracle and Azure SQL Edge were returned to their original stopped state. Existing user installations, saved connections and unrelated running databases were left untouched.
