---
name: sqlx
description: Manage encrypted database connections and execute SQL with the OtterMind SQLX CLI. Use for MySQL, MariaDB, TiDB, GreatSQL, OceanBase, PostgreSQL, CockroachDB, YugabyteDB, openGauss, Oracle, SQL Server, ClickHouse, Trino, StarRocks, Apache Doris, TDengine, Dameng, KingbaseES, Redis, and MongoDB connection checks, queries, DDL, and schema inspection. This skill targets OtterMind/sqlx, not the Rust SQLx migration CLI.
metadata:
  cli-compat: ">=0.1.11, <0.2.0"
---

# SQLX

Operate databases through the OtterMind SQLX executable. Confirm it with `sqlx --version` and `sqlx --help` before the first call; when it is absent or belongs to another project, read `references/cli.md`. Load only the reference the current task needs.

```sh
sqlx datasource list
sqlx sql execute --datasource <id> --command "SELECT 1" --command "SELECT 2"
```

## Read the reference for the task

| Situation | Reference |
|---|---|
| Installing or repairing the CLI, PATH, updates, or managing this Skill | `references/cli.md` |
| Listing, creating, editing or testing a datasource, TLS choices, engine-specific fields | `references/connections.md` |
| The user types the password in a browser, or pages and UI plugins are involved | `references/local-ui.md` |
| Before any statement that is not clearly read-only, including `--view`, manual Refresh and automatic refresh | `references/approval.md` |
| Execution semantics, output fields, partial or unknown outcomes, retries | `references/execution.md` |
| A first command is waiting on a download, or components are prefetched | `references/downloads.md` |
| Writing SQL for the connected engine | the recipe of that engine, below |

## Database recipes

Load the recipe of the connected datasource, and only that one. Each recipe links the vendor's own documentation for its operations; match the connected server version.

MySQL `references/mysql.md` · MariaDB `references/mariadb.md` · TiDB `references/tidb.md` · GreatSQL `references/greatsql.md` · OceanBase `references/oceanbase.md` · PostgreSQL `references/postgresql.md` · CockroachDB `references/cockroachdb.md` · YugabyteDB `references/yugabytedb.md` · openGauss `references/opengauss.md` · Oracle `references/oracle.md` · SQL Server `references/sqlserver.md` · ClickHouse `references/clickhouse.md` · Trino `references/trino.md` · StarRocks `references/starrocks.md` · Apache Doris `references/doris.md` · TDengine `references/tdengine.md` · Dameng `references/dameng.md` · KingbaseES `references/kingbase.md` · Redis `references/redis.md` · MongoDB `references/mongodb.md`

TiDB, GreatSQL, OceanBase, StarRocks and Apache Doris reuse the MySQL worker, and YugabyteDB reuses the PostgreSQL worker; ClickHouse needs its HTTP port, Trino needs `--database <catalog>[.<schema>]`, and the remaining engines, including TDengine, openGauss, Dameng and KingbaseES, connect through the JDBC worker. Redis and MongoDB are not SQL: each `--command` is one Redis command or one MongoDB command document, and `references/approval.md` lists the write commands for both.

## Rules that always apply

- Read `references/approval.md` before running any state-changing or unknown statement; a one-time approval never covers a later rerun.
- Let the user enter credentials in the browser with `sqlx datasource add --ui ...`. Never ask for a password in the conversation, and never put secrets in arguments or logs.
- After any command that opens a page, put the complete returned `url` in a clickable Markdown link in the reply.
- Keep TLS verification on; `--tls disable` is only for a connection that is explicitly unencrypted.
- Workers and drivers arrive on first use; tell the user a first command can be waiting on that download instead of reporting it as a hang.
