SQLX 0.1.10 adds MariaDB, CockroachDB, ClickHouse and Trino.

## New

- `--type mariadb` connects to MariaDB and `--type cockroachdb` (aliases `cockroach` and `crdb`) connects to CockroachDB. Both speak the protocol of an existing worker: MariaDB reuses the MySQL worker and CockroachDB the PostgreSQL worker, so `mysql` and `postgresql` datasources keep working against them. New connections default to ports 3306 and 26257.
- `--type clickhouse` connects to ClickHouse on its HTTP port (8123 by default) and `--type trino` connects to Trino on 8080, each through its official JDBC driver. Trino takes `--database <catalog>[.<schema>]` and requires a username; a password is only sent when TLS is enabled, and writes are limited to writable catalogs. The ClickHouse driver rejects a result set whose columns share a label, so a query returning duplicate aliases fails there while every other engine keeps the labels distinct.
- `sqlx prefetch` accepts the new names. `mariadb` fetches the MySQL worker, `cockroachdb` the PostgreSQL worker, and `clickhouse` or `trino` the JDBC runner, the pinned JRE and the engine's driver jars.

## Improvements

- A JDBC component can ship several jars. The ClickHouse component includes the driver's logging dependency, so it runs without extra setup.
- The Skill carries one reference file per engine (`mysql`, `mariadb`, `postgresql`, `cockroachdb`, `oracle`, `sqlserver`, `clickhouse`, `trino`) instead of a shared file for the newer engines. Each lists its operations with the purpose, the SQL, the expected result and a link to the vendor's documentation.
- The local connection page from `sqlx datasource add --ui` and the saved-datasource view list all eight types, so MariaDB, CockroachDB, ClickHouse and Trino connections can be created in the browser like the existing ones.
- `sqlx mcp` advertises every component `sqlx prefetch` accepts.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.10 Skill requires SQLX 0.1.10.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.9...v0.1.10
