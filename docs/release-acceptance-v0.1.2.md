# SQLX v0.1.2 release acceptance

Date: 2026-09-11. [v0.1.2](https://github.com/OtterMind/sqlx/releases/tag/v0.1.2) is published as the latest stable release. Public-download acceptance passed on macOS ARM64.

## Source and artifacts

- Annotated tag source: `9769cf3e0b3561c7a9ac09c620f17d4e738cb89d`. Its annotation records the source, packaging workflow commit/ref, all five platforms and `publish=true`.
- [Source CI](https://github.com/OtterMind/sqlx/actions/runs/34572162986): all nine jobs passed, including updater replacement/recovery on five platforms and MySQL, PostgreSQL, Oracle and SQL Server integration.
- [Packaging and publication](https://github.com/OtterMind/sqlx/actions/runs/34572797809): all seven jobs passed. Both macOS targets completed Developer ID signing and Apple notarization. The workflow verified the annotated tag and successful source CI before publishing.
- All 28 public files were downloaded independently. The 27 SHA256SUMS entries, GitHub asset digests and manifest package hashes matched. All 25 ZIPs passed integrity checks.
- All 20 native executables matched their declared architecture. All eight macOS executables passed independent Developer ID signature verification.
- The manifest contains 30 components: 20 native executables, five shared packages and five pinned upstream JRE downloads. Non-JRE components point to fixed v0.1.2 URLs. GitHub Latest and `release-version.txt` both identify 0.1.2.

## Public installation and update commands

The main executable, PostgreSQL worker, UI service, default plugin and managed Skills came from public release URLs. These installation and browser checks used no development worker, custom component manifest or Java override. Separate install/data/Skill paths isolated existing user installations.

| Scenario | Result |
|---|---|
| README upgrade | Installed genuine public 0.1.1, then reran the public README installer without a version override and received 0.1.2 |
| Saved data | The master key, device identity and encrypted datasource file remained byte-for-byte unchanged after upgrading and after update-command checks |
| Existing connection | The PostgreSQL connection saved by 0.1.1 still connected; two SQL arguments returned the database name and exact integer `9007199254740993` through the public 0.1.2 worker |
| Update check | Returned current/latest version 0.1.2 and `up_to_date` from the official release source |
| Equal-version install | `update install --version 0.1.2` returned `up_to_date`; the executable hash did not change |
| Update history | `update status` returned the saved check and installation outcomes |
| Managed Skill update | Updated the existing 0.1.1 Skill to 0.1.2, including the update command reference |
| Skill-first installation | Extracted the public Skill, followed its installation guide using the verified CLI archive, then verified help/version/init and a separate managed Skill installation |
| Initial footprint | Fresh initialization created only encrypted storage, key, identity and lock files; it downloaded no database worker or JRE |

Version 0.1.1 does not contain self-update commands. Its first upgrade used the standalone installer, not `update install`.

## Real executable replacement and recovery

After public installation, `tests/updates.py` ran again using disposable copies of the **published macOS ARM64 executable** as the updater. A local HTTP release fixture supplied native candidate executables representing a future 0.1.3. This exercises actual replacement without claiming that a future official release exists.

The published executable passed checks for cache/status behavior, noninteractive exclusion, terminal-triggered detached checks, no-op/downgrade behavior, corrupt checksums, unsafe archives, wrong product identity, a candidate that hangs, and a candidate that succeeds while staged but exits unsuccessfully at the installed path. Failed installation restored the original executable and its exact hash. Successful replacement returned the expected version at the final path. Encrypted-data sentinels remained unchanged.

Concurrent update attempts were rejected while the first update proceeded. An older CLI invocation waiting on stdin stayed alive and completed after executable replacement. The source CI ran the same suite on all five supported platforms, including Windows helper cleanup and restoration. The five-second candidate startup limit remains in force.

## Public browser workflows

Playwright CLI used an isolated headless browser and publicly downloaded services/plugins. The user's existing preview window and saved MySQL connection were preserved.

- The default plugin reported version 0.1.2. The SQL preview started collapsed, and a 251-row result plus a second result set loaded successfully.
- Pagination reached the final 51 rows, disabled Next at the end, and preserved exact integers, NULLs and HTML-like values as text. Full-cell inspection and clipboard copy preserved `9007199254740993` exactly.
- Result-set selection worked with mouse and keyboard. The theme icon switched between light and dark; the preference survived reload. Workspace/result navigation retained the same document rather than reloading the page.
- A separate fresh installation opened a prefilled PostgreSQL credential form. Incorrect credentials produced failure feedback without saving a datasource. Correcting the password and submitting with Enter connected and saved it; completion survived reload and the saved connection passed a CLI connection test.
- Setup-status responses omitted passwords. The fixture password was absent from plaintext files under the fresh data directory. Cancelling another setup request saved no additional datasource.
- Browser console output contained no errors or warnings.
- A dedicated PostgreSQL sequence remained at 251 after pagination, result-set selection, reload, theme changes and workspace navigation, proving that these page operations did not rerun the SQL.

## Limits and cleanup

Public executable/browser acceptance ran on macOS ARM64. The other platforms were covered by source CI and artifact verification; the full public-download browser flow was not repeated interactively on every platform. Database tests used isolated fixtures and explicitly disabled TLS. This release does not add unattended installation or telemetry.

The current workbench sidebar lists connection setup requests and query history. It does **not** yet list saved datasources or provide a general datasource-management page. A zero connection-request count is not the number of saved connections. The tested PostgreSQL authentication failure currently displays the generic `db error` message.

The acceptance sequence was removed, both acceptance-owned UI services stopped, and the isolated headless browser closed. Temporary acceptance installations/downloads were removed after recording the results. The existing user preview, its encrypted MySQL connection and its supporting PostgreSQL fixture were retained.
