# SQLX v0.1.0 release acceptance

Date: 2026-09-10. Result: the published release passed the end-to-end scenarios below on macOS ARM64.

## Published release

- [Release v0.1.0](https://github.com/OtterMind/sqlx/releases/tag/v0.1.0), published as a regular release at 13:04:25 UTC.
- Release source: `e9d36aedbdefb241a0e044fb962714e05a3c7a8f`.
- [Release-source CI](https://github.com/OtterMind/sqlx/actions/runs/34479769468): all nine jobs passed, including the five supported platforms and database integration tests.
- [Signed release build](https://github.com/OtterMind/sqlx/actions/runs/34479984482): all five native platform jobs, shared packages, and assembly/publication passed.
- An annotated `v0.1.0` tag records the source, workflow commit, publish input, and platform matrix.
- All 22 release assets are public: 19 independently downloadable ZIP files, `manifest.json`, `SHA256SUMS`, and `release-version.txt`.

## Black-box method

Acceptance used an empty installation directory and empty user data directory for each entry path. Instructions were fetched from the public GitHub README and the public/source Skill. The executable came from the published release. Native workers, JDBC runtime, vendor drivers, and Skill were downloaded by the installed CLI through its default public manifest.

The tests used documented install-directory, data-directory, and Skill-path options to isolate them from the user's existing environment. They did not use a source build, local worker directory, custom manifest, development Java override, or the CI release-bundle as an installation shortcut.

The first installer attempt encountered a GitHub TLS connection error. Retrying the same public installer succeeded. This was recorded as an environmental interruption; no product code or release assets were changed to bypass it.

## Installation and local state

| Scenario | Observed result |
|---|---|
| README → CLI installation | Public installer downloaded and verified the macOS ARM64 release executable; version identified `0.1.0 (OtterMind/sqlx)` |
| Help and initialization | Documented command groups were available; initialization created local state |
| Repeated initialization | The installation identity and encryption key remained unchanged |
| Initial dependency footprint | No database worker or JRE was downloaded by initialization |
| CLI → Skill installation | `skill install --path ...` downloaded the published Skill and all references |
| Skill status and update | Status reported an intact installation; a compatible update succeeded |
| Skill content | All Skill files were English; each database operation included purpose, placeholders, expected result, and official documentation links |
| Skill → CLI installation | Skill was installed before the CLI; its guide was used to select, download, hash-check, and install the release executable, initialize it, and query PostgreSQL |
| Local Skill modifications | Updating a modified managed Skill failed without overwriting the edit; restoring the original allowed the update |
| Encrypted persistence | Credentials were absent from public datasource responses and plaintext storage; key and encrypted datasource files had mode `0600` on the tested host |
| Database-worker caching | A second connection reused installed components without another download |
| JDBC dependency download | The CLI downloaded the private Temurin JRE, JDBC runner, Oracle driver, and SQL Server driver as separate components |

## Database coverage

The local acceptance targets were existing Docker databases explicitly authorized for testing. Only uniquely named test databases, schemas, tables, views, indexes, and routines were created. The SQL Server-family local target was Azure SQL Edge; the separate CI integration suite also passed against SQL Server 2022.

| Database | Backend used by the release | Skill operations executed | Result |
|---|---|---:|---|
| MySQL 8.4 | Downloaded native worker | 10 | Passed |
| PostgreSQL 17 | Downloaded native worker | 10 | Passed |
| Oracle Free 23 | Downloaded JDBC runner, private JRE, and Oracle driver | 8 | Passed |
| Azure SQL Edge | Downloaded JDBC runner, private JRE, and Microsoft SQL Server driver | 9 | Passed |

All 37 SQL operation blocks in the four published Skill references were executed after replacing their documented placeholders with test-object names. This included connection context, database/schema discovery where supported, tables, columns, indexes, table DDL where the database exposes it, and view/routine definitions where documented. The documented PostgreSQL and SQL Server table-DDL limitations remain applicable.

For every database, acceptance also covered:

- Datasource creation with the documented environment-variable credential input, listing, inspection, update, rename with a stable ID, connection test, and deletion.
- Duplicate-name and invalid-port rejection without damaging existing saved configuration.
- Wrong-password failure, credential correction, and successful reconnection.
- DDL, insertion, and queries against isolated objects.
- Exact large-integer values, decimal values, duplicate column labels, NULL, and binary data.
- A multi-statement call where an earlier insert succeeds, the next statement fails, and the final statement is skipped. A subsequent call confirmed the earlier autocommitted write and absence of the skipped write.
- Complete output of a value larger than 1 MB. PostgreSQL additionally returned all 100,005 requested rows.
- Removal of test objects and datasources, followed by a fresh call confirming the saved datasource was absent.

MySQL and SQL Server-family execution returned multiple result sets. MySQL, PostgreSQL, and SQL Server temporary tables remained available within one invocation and were unavailable in the next. Interrupting a PostgreSQL sleep query returned a structured failure, did not execute the next statement, and left subsequent CLI connections usable.

## Artifact verification

The published assets were downloaded separately for inspection:

- All 21 files listed in `SHA256SUMS` matched their hashes.
- All 19 ZIP assets matched the component manifest and passed archive integrity checks.
- All 15 native executables had the expected binary architecture across the five platform targets.
- All six macOS native executables passed independent Developer ID signature verification; both macOS package jobs completed Apple notarization.
- The manifest contained 24 component entries: 15 native artifacts, four shared artifacts, and five pinned upstream JRE downloads.

## Limits and cleanup

The interactive public-release black-box run was performed on macOS ARM64. Five-platform compilation, command behavior, installer checks, and Skill distribution checks were separately covered by CI. This report does not claim the complete public-download/database workflow was repeated on every operating system.

Local database tests used username/password authentication with TLS disabled for the authorized local fixtures. They do not establish a full certificate, integrated-authentication, or historical database-version compatibility matrix. PostgreSQL types exposed as Base64 and the documented lack of persistent sessions, SQL-file input, automatic updates, and telemetry remain unchanged.

All test-created database objects and saved test datasources were removed. Oracle and Azure SQL Edge were restored to their initial stopped state; the pre-existing MySQL and PostgreSQL containers remained running. Temporary credential files and test installation directories were removed after recording the results.
