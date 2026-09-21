---
name: sqlx
description: Manage encrypted database connections and execute SQL with the OtterMind SQLX CLI. Use for MySQL, MariaDB, TiDB, PostgreSQL, CockroachDB, YugabyteDB, Oracle, SQL Server, ClickHouse, Trino, StarRocks, and Apache Doris connection checks, queries, DDL, and schema inspection. This skill targets OtterMind/sqlx, not the Rust SQLx migration CLI.
metadata:
  cli-compat: ">=0.1.11, <0.2.0"
---

# SQLX

Use the OtterMind SQLX executable for database operations. First check `sqlx --version` and `sqlx --help`. If it is missing or belongs to another project, follow [CLI installation](references/install-cli.md). Do not install an unrelated `sqlx-cli` package from Cargo or npm.

Installation puts the executable in `~/.local/bin` on macOS and Linux, or `%LOCALAPPDATA%\Programs\SQLX` on Windows. That directory must be on PATH; the `npx` installer prints the exact line to add when it is missing, and the shell installers print the directory. A freshly installed CLI is not resolvable in a shell whose PATH was built earlier, so export the directory or call the absolute path instead of reinstalling.

## Connect

- Run `sqlx init` when initialization is needed. It creates a local key and device identity; it does not upload device information.
- Run `sqlx datasource list` and select the intended saved datasource. `show --id <id>` displays its nonsecret configuration.
- When the user needs to supply credentials, prefer a prefilled local page: `sqlx datasource add --ui --name <name> --type <type> --host <host> --port <port> --database <database>`. Oracle uses `--service`. The user enters credentials directly in the browser; do not ask them to paste a password into the conversation. See [local pages](references/local-ui.md).
- The UI command returns `request_id`, `url`, and `waiting_for_user` immediately. Share the link and resume after the user saves. Check `sqlx datasource setup-status --request-id <id>` and use the returned datasource ID. Handle cancellation, expiry, and failed connection feedback without inventing a completed connection.
- Create connections with `sqlx datasource add --name <name> --type <type> --host <host> --port <port> --database <database> --username-env <variable> --password-env <variable>`. Oracle uses `--service <service-name>`. Environment flags take variable names, never literal secrets.
- For structured noninteractive input, `--connection-stdin` accepts the connection object documented in the repository README. Do not place credentials in command arguments or logs.
- TLS defaults to certificate verification. Use `--tls disable` only when that connection is explicitly intended to be unencrypted, such as the isolated test fixtures.
- Test with `sqlx datasource test --id <id>`. Required workers and Java dependencies download automatically from the compatible release manifest.
- More engines are supported without a separate driver: TiDB, StarRocks and Apache Doris speak the MySQL protocol and reuse the MySQL worker, while YugabyteDB speaks the PostgreSQL protocol and reuses the PostgreSQL worker. ClickHouse needs its HTTP port, and Trino needs `--database <catalog>[.<schema>]` with a user but no password unless TLS is enabled. Load the matching recipe from [database recipes](#database-recipes).

## Execute

```text
sqlx sql execute --datasource <id> --sql "SELECT 1" --sql "SELECT 2"
```

Each `--sql` is a complete driver statement. A call uses one connection and executes statements in order. The initial mode is autocommit, and the first error stops the remaining statements. A batch is not automatically atomic. There is no SQL-file input, client-script interpreter, or cross-call session. Put operations requiring a temporary table or session variable in the same call. Do not submit `GO`, `DELIMITER`, or psql backslash commands as SQL.

### Approval before state-changing SQL

SQLX can submit arbitrary SQL accepted by the selected database and account. The CLI does not provide a read-only safety gate, and the first keyword is not a reliable way to classify a statement. This is an Agent workflow rule, not a database permission mechanism; a user or program invoking the CLI directly bypasses it. Before starting a call, inspect every `--sql` statement in order and classify the whole batch as read-only, state-changing, or unknown.

- Treat ordinary `SELECT`, `SHOW`, `DESCRIBE`/`DESC`, and `EXPLAIN` as read-only only when the complete statement has no write-capable function, data-changing CTE, `SELECT INTO`, locking clause, or other vendor-specific side effect.
- Treat `INSERT`, `UPDATE`, `DELETE`, `MERGE`, `REPLACE`, `CREATE`, `ALTER`, `DROP`, `TRUNCATE`, `RENAME`, `GRANT`, `REVOKE`, transaction-control statements, session-changing statements, administrative commands, and maintenance commands as state-changing. Include statements that can change schema, permissions, session state, metadata, statistics, or other database state even when they do not change table rows.
- Treat `CALL`, `EXEC`, `DO` blocks, dynamic SQL, stored procedures/functions with unknown behavior, vendor commands, and statements whose effect cannot be established from the supplied SQL as unknown. Do not infer that a `SELECT` is harmless from its leading keyword alone.

For a state-changing or unknown batch, explain before execution: the target datasource and database/schema, the exact statements or a faithful summary, the expected state or permission changes, the likely scope, and that SQLX starts in autocommit without an implicit all-or-nothing transaction. If the user has not explicitly authorized that operation and scope, pause and ask for confirmation. A request to inspect, explain, draft, or find a problem does not authorize a write. A direct request to execute a specific statement against a named target is explicit authorization; do not ask the same confirmation again, but still state the side effect before running it.

Establish the blast radius with read-only SQL before asking: inspect the schema with the recipe for the connected database, and reach the affected rows through the same predicate with `SELECT COUNT(*)` or `EXISTS`. Report those findings instead of describing the change as small. A statement without a limiting predicate reaches the whole table, and a committed batch cannot be undone through this CLI: `DROP`, `TRUNCATE`, and a data-discarding `ALTER` are irreversible, so name the specific operation instead of approving "a change".

Complete this check for the entire batch before executing any statement. Do not run a read-only prefix and then wait before an unapproved write, because earlier statements may already have committed. If the user approves only part of a batch, prepare a separate command and show the changed statement list for confirmation; do not silently rewrite or reorder the original SQL.

The same approval gate applies to `--view`, manual result Refresh, and automatic refresh. A one-time approval does not authorize future reruns of a state-changing or unknown batch. Keep automatic refresh disabled unless the user explicitly approves repeated execution, its interval, and its side effects. Reloading, pagination, reconnecting, and returning to a page read cached results and do not authorize a new SQL execution.

Output is a complete JSON object with an ordered `events` array. `columns` describes a result, `row` carries positional values, and `result_end` contains row/update counts. Duplicate column names are preserved. Numeric values use strings to retain precision; binary values use Base64. Some PostgreSQL types without a text decoder retain their wire value as Base64 with the database type. Use explicit SQL casts when human-readable text is preferable.

Check both the top-level `success` and process exit status. `error` and `skipped` events identify partial progress. An incomplete response, timeout, or `outcome: unknown` does not prove a write failed; inspect database state before deciding whether to retry. Do not replay the entire batch automatically. Prior successful statements may already have committed.

After a state-changing execution, report the actual target, statement outcome, affected-row or result information provided by the driver, and any partial or unknown outcome. Offer a focused read-only verification when useful, but do not issue an unrequested compensating write or claim that a failed client response rolled back database changes.

The CLI does not truncate results. Limit exploratory queries in SQL to avoid overflowing the agent's own output budget. Do not assume an agent tool showing only part of stdout means the database returned only those rows.

## Downloads on first use

The CLI does not embed database drivers, the JDBC runtime or the browser UI. The first operation that needs one downloads it from the fixed GitHub Release into the SQLX data directory and reuses it afterwards:

- MySQL and PostgreSQL workers (~5 MB) on the first query for that database type.
- The JDBC runner, the pinned JRE and the vendor driver on the first Oracle or SQL Server query; the JRE is the largest download.
- The UI engine (~6 MB) and the default UI plugin on the first `--view`, `datasource add/update --ui` or `sqlx ui`.

Each download prints `Downloading …`, a progress line with speed and estimated time, and a final `Downloaded … in 12.3s (390 KB/s)` line on stderr. An interrupted transfer is retried up to three times. Tell the user that a first command can wait for a download instead of reporting it as a hang, and re-run the same command after a failure: components that are already installed are reused.

When the network is slow, prefetch ahead of time with `sqlx prefetch <component>`, where the component is `mysql`, `postgres`, `oracle`, `sqlserver`, `ui`, `skill` or `all` (`all` includes the JDBC runtime and JRE).

## Database recipes

Load only the reference for the selected datasource:

- [MySQL](references/mysql.md): databases, tables, DDL, indexes, limits.
- [MariaDB](references/mariadb.md): MySQL-compatible SQL, authentication defaults, storage engines.
- [TiDB](references/tidb.md): MySQL compatibility, regions, statistics, explain plans.
- [PostgreSQL](references/postgresql.md): databases, schemas, catalog inspection, DDL boundaries.
- [CockroachDB](references/cockroachdb.md): PostgreSQL wire compatibility, cluster metadata, ranges.
- [YugabyteDB](references/yugabytedb.md): YSQL, cluster nodes, indexes, sessions.
- [Oracle](references/oracle.md): service context, owners, metadata, DDL.
- [SQL Server](references/sqlserver.md): databases, schemas, catalog inspection, DDL boundaries.
- [ClickHouse](references/clickhouse.md): HTTP protocol, MergeTree DDL, columnar inspection, label limits.
- [Trino](references/trino.md): three-part names, catalogs and connectors, read-only vs writable sources.
- [StarRocks](references/starrocks.md): catalogs, key models, backends, partitions and tablets.
- [Apache Doris](references/doris.md): catalogs, data models, backends, table status.

Each operation includes its purpose, placeholder replacements, expected result, and links to the corresponding official documentation. When syntax, version behavior, permissions, or a returned field needs clarification, use available web tools to open that operation's official link and select the connected server's version. If web access is unavailable, use the bundled recipe and state that the current official page was not checked. Keep vendor client commands such as `GO` and `DELIMITER` separate from SQL accepted by this CLI.

## Show results to the user

After every successful page-opening command (`ui`, `datasource add/update --ui`, or `sql execute --view`), immediately put the **complete returned `url` in a clickable Markdown link in the user-facing reply**, even if the browser was opened automatically. Include it again in the final reply when that is the delivery message. Do not finish with only "the page is open", a request/result ID, a screenshot, or tool output. Preserve the returned path. Local pages establish their browser session automatically; do not add an authorization fragment. For example: `Open the page: [SQLX workspace](<complete returned url>)`. Use the actual CLI response, never a guessed address or this placeholder.

Local page URLs can be reopened without an authorization deadline. Run `sqlx ui --no-open` to start or locate the workspace without replaying SQL; its saved local port is reused after restart; the user can select the retained result in query history. With `--no-open`, say the page is ready and provide the link rather than claiming a browser was launched.

When the user wants to inspect data visually, append `--view` to the already prepared SQL execution command. Apply the approval gate above before starting the query. It returns a local result URL and starts the query once in the local service. The user does not need to paste or rerun the SQL. Browser reload and pagination read the same cached execution. In SQLX 0.1.3, the page's **Refresh** action explicitly reruns all original SQL statements in order; apply the approval gate again before a state-changing or unknown refresh. The optional refresh interval does the same; do not enable it unless the user explicitly approves repeated execution and its side effects. SQL is not classified or rewritten, so writes in the original batch run again too. UI results are retained locally for 24 hours; do not automatically rerun expired or interrupted queries. `--no-open` returns a link without launching a browser. These pages open on the machine running SQLX. See [local pages](references/local-ui.md) for credential editing, UI plugin installation and selection, lifecycle, and limitations.

Skill installation and updates use `sqlx skill install --target codex`, `--target claude`, `--target dsh`, `--target pi`, or `--path <skill-directory>`, followed by the target agent's discovery/reload mechanism. Codex and dsh share `~/.agents/skills`; Pi uses `~/.pi/agent/skills`. `sqlx skill status` and `sqlx skill update` operate on SQLX-managed installations and preserve locally modified skill files. `status` marks a record whose directory no longer exists with `missing: true`, and `sqlx skill remove --path <skill-directory>` stops managing that installation without deleting any files.

## Check and install CLI updates

Use `sqlx update check` to fetch the latest stable version without installing it. `sqlx update install` changes the executable when the user requests an upgrade; `--version <version>` selects an exact stable release. `sqlx update status` reads the last check/installation outcome without networking. An update check alone is not authorization to install.

An `installed` result means the new binary was verified at the installed path. A failure must be reported as a failure; inspect `status` and any retained backup information instead of repeating replacement blindly. Updating never replays SQL or replaces saved datasource files. Update the managed Skill separately with `sqlx skill update`, then follow the agent's reload/discovery behavior. See [CLI update details](references/updates.md).
