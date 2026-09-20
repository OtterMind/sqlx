SQLX 0.1.8 fixes credential redaction in the JDBC worker and documents how to connect to a local database.

## Fixes

- The JDBC worker redacted the username and password wherever they appeared, so a driver message could lose unrelated text: with the password `oracle`, the Oracle help link arrived as `https://docs.[redacted].com/error-help/db/ora-17002/`. Redaction now leaves a secret alone when it is only part of a word or a hostname, matching the CLI behaviour shipped in 0.1.7.

## Improvements

- The README's first-connection section now names the MySQL, Oracle and SQL Server flags, lists the three failure messages a local container produces under the default certificate verification, and shows the `--tls disable` form for a new or existing connection.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.8 Skill requires SQLX 0.1.8.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.7...v0.1.8
