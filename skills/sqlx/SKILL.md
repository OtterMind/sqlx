---
name: sqlx
description: Manage encrypted database connections and execute SQL with the OtterMind SQLX CLI. Use for MySQL, PostgreSQL, Oracle, and SQL Server connection checks, queries, DDL, and schema inspection. This skill targets OtterMind/sqlx, not the Rust SQLx migration CLI.
metadata:
  cli-compat: ">=0.1.2, <0.2.0"
---

# SQLX

Use the OtterMind SQLX executable for database operations. First check `sqlx --version` and `sqlx --help`. If it is missing or belongs to another project, follow [CLI installation](references/install-cli.md). Do not install an unrelated `sqlx-cli` package from Cargo or npm.

## Connect

- Run `sqlx init` when initialization is needed. It creates a local key and device identity; it does not upload device information.
- Run `sqlx datasource list` and select the intended saved datasource. `show --id <id>` displays its nonsecret configuration.
- When the user needs to supply credentials, prefer a prefilled local page: `sqlx datasource add --ui --name <name> --type <type> --host <host> --port <port> --database <database>`. Oracle uses `--service`. The user enters credentials directly in the browser; do not ask them to paste a password into the conversation. See [local pages](references/local-ui.md).
- The UI command returns `request_id`, `url`, and `waiting_for_user` immediately. Share the link and resume after the user saves. Check `sqlx datasource setup-status --request-id <id>` and use the returned datasource ID. Handle cancellation, expiry, and failed connection feedback without inventing a completed connection.
- Create connections with `sqlx datasource add --name <name> --type <type> --host <host> --port <port> --database <database> --username-env <variable> --password-env <variable>`. Oracle uses `--service <service-name>`. Environment flags take variable names, never literal secrets.
- For structured noninteractive input, `--connection-stdin` accepts the connection object documented in the repository README. Do not place credentials in command arguments or logs.
- TLS defaults to certificate verification. Use `--tls disable` only when that connection is explicitly intended to be unencrypted, such as the isolated test fixtures.
- Test with `sqlx datasource test --id <id>`. Required workers and Java dependencies download automatically from the compatible release manifest.

## Execute

```text
sqlx sql execute --datasource <id> --sql "SELECT 1" --sql "SELECT 2"
```

Each `--sql` is a complete driver statement. A call uses one connection and executes statements in order. The initial mode is autocommit, and the first error stops the remaining statements. A batch is not automatically atomic. There is no SQL-file input, client-script interpreter, or cross-call session. Put operations requiring a temporary table or session variable in the same call. Do not submit `GO`, `DELIMITER`, or psql backslash commands as SQL.

Output is a complete JSON object with an ordered `events` array. `columns` describes a result, `row` carries positional values, and `result_end` contains row/update counts. Duplicate column names are preserved. Numeric values use strings to retain precision; binary values use Base64. Some PostgreSQL types without a text decoder retain their wire value as Base64 with the database type. Use explicit SQL casts when human-readable text is preferable.

Check both the top-level `success` and process exit status. `error` and `skipped` events identify partial progress. An incomplete response, timeout, or `outcome: unknown` does not prove a write failed; inspect database state before deciding whether to retry. Do not replay the entire batch automatically. Prior successful statements may already have committed.

The CLI does not truncate results. Limit exploratory queries in SQL to avoid overflowing the agent's own output budget. Do not assume an agent tool showing only part of stdout means the database returned only those rows.

## Database recipes

Load only the reference for the selected datasource:

- [MySQL](references/mysql.md): databases, tables, DDL, indexes, limits.
- [PostgreSQL](references/postgresql.md): databases, schemas, catalog inspection, DDL boundaries.
- [Oracle](references/oracle.md): service context, owners, metadata, DDL.
- [SQL Server](references/sqlserver.md): databases, schemas, catalog inspection, DDL boundaries.

Each operation includes its purpose, placeholder replacements, expected result, and links to the corresponding official documentation. When syntax, version behavior, permissions, or a returned field needs clarification, use available web tools to open that operation's official link and select the connected server's version. If web access is unavailable, use the bundled recipe and state that the current official page was not checked. Keep vendor client commands such as `GO` and `DELIMITER` separate from SQL accepted by this CLI.

## Show results to the user

When the user wants to inspect data visually, append `--view` to the already prepared SQL execution command. It returns a local result URL and starts the query once in the local service. The user does not need to paste or rerun the SQL. Refreshing or paging the result reads the same cached execution. UI results are retained locally for 24 hours; do not automatically rerun expired or interrupted queries. `--no-open` returns a link without launching a browser. These pages open on the machine running SQLX. See [local pages](references/local-ui.md) for credential editing, UI plugin installation and selection, lifecycle, and limitations.

Skill installation and updates use `sqlx skill install --target codex`, `--target claude`, or `--path <skill-directory>`, followed by the target agent's discovery/reload mechanism. `sqlx skill status` and `sqlx skill update` operate on SQLX-managed installations and preserve locally modified skill files.

## Check and install CLI updates

Use `sqlx update check` to fetch the latest stable version without installing it. `sqlx update install` changes the executable when the user requests an upgrade; `--version <version>` selects an exact stable release. `sqlx update status` reads the last check/installation outcome without networking. An update check alone is not authorization to install.

An `installed` result means the new binary was verified at the installed path. A failure must be reported as a failure; inspect `status` and any retained backup information instead of repeating replacement blindly. Updating never replays SQL or replaces saved datasource files. Update the managed Skill separately with `sqlx skill update`, then follow the agent's reload/discovery behavior. See [CLI update details](references/updates.md).
