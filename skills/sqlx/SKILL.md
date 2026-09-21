---
name: sqlx
description: Manage encrypted database connections and execute SQL with the OtterMind SQLX CLI. Use for MySQL, MariaDB, TiDB, PostgreSQL, CockroachDB, YugabyteDB, Oracle, SQL Server, ClickHouse, Trino, StarRocks, and Apache Doris connection checks, queries, DDL, and schema inspection. This skill targets OtterMind/sqlx, not the Rust SQLx migration CLI.
metadata:
  cli-compat: ">=0.1.11, <0.2.0"
---

# SQLX

Operate databases through the OtterMind SQLX executable. Confirm it with `sqlx --version` and `sqlx --help` before the first call; when it is absent or belongs to another project, read `references/install-cli.md`. Load only the reference the current task needs.

```sh
sqlx datasource list
sqlx sql execute --datasource <id> --sql "SELECT 1" --sql "SELECT 2"
```

## Read the reference for the task

| Situation | Reference |
|---|---|
| The CLI is missing, belongs to another project, or needs a PATH repair | `references/install-cli.md` |
| Checking or installing a CLI update, or inspecting a failed update | `references/updates.md` |
| Listing, creating, editing or testing a datasource, TLS choices, engine-specific fields | `references/connections.md` |
| The user types the password in a browser, or pages and UI plugins are involved | `references/local-ui.md` |
| Before any statement that is not clearly read-only, including `--view`, manual Refresh and automatic refresh | `references/approval.md` |
| Execution semantics, output fields, partial or unknown outcomes, retries | `references/execution.md` |
| A first command is waiting on a download, or components are prefetched | `references/downloads.md` |
| Writing SQL for the connected engine | the recipe of that engine, below |

## Database recipes

Load the recipe of the connected datasource, and only that one:

MySQL `references/mysql.md` · MariaDB `references/mariadb.md` · TiDB `references/tidb.md` · PostgreSQL `references/postgresql.md` · CockroachDB `references/cockroachdb.md` · YugabyteDB `references/yugabytedb.md` · Oracle `references/oracle.md` · SQL Server `references/sqlserver.md` · ClickHouse `references/clickhouse.md` · Trino `references/trino.md` · StarRocks `references/starrocks.md` · Apache Doris `references/doris.md`

TiDB, StarRocks and Apache Doris reuse the MySQL worker, and YugabyteDB reuses the PostgreSQL worker; ClickHouse needs its HTTP port, and Trino needs `--database <catalog>[.<schema>]`.

## Rules that always apply

- Read `references/approval.md` before running any state-changing or unknown statement; a one-time approval never covers a later rerun.
- Let the user enter credentials in the browser with `sqlx datasource add --ui ...`. Never ask for a password in the conversation, and never put secrets in arguments or logs.
- After any command that opens a page, put the complete returned `url` in a clickable Markdown link in the reply.
- Keep TLS verification on; `--tls disable` is only for a connection that is explicitly unencrypted.
- Workers and drivers arrive on first use; tell the user a first command can be waiting on that download instead of reporting it as a hang.
