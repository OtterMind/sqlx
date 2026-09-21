# Approval before state-changing SQL

Read this before running anything that is not clearly read-only. SQLX submits arbitrary SQL accepted by the selected database and account, provides no read-only safety gate, and the first keyword is not a reliable classifier. This is an Agent workflow rule, not a database permission mechanism; a user or program invoking the CLI directly bypasses it.

Before starting a call, inspect every `--sql` statement in order and classify the whole batch as read-only, state-changing, or unknown.

- Treat ordinary `SELECT`, `SHOW`, `DESCRIBE`/`DESC`, and `EXPLAIN` as read-only only when the complete statement has no write-capable function, data-changing CTE, `SELECT INTO`, locking clause, or other vendor-specific side effect.
- Treat `INSERT`, `UPDATE`, `DELETE`, `MERGE`, `REPLACE`, `CREATE`, `ALTER`, `DROP`, `TRUNCATE`, `RENAME`, `GRANT`, `REVOKE`, transaction-control statements, session-changing statements, administrative commands, and maintenance commands as state-changing. Include statements that can change schema, permissions, session state, metadata, statistics, or other database state even when they do not change table rows.
- Treat `CALL`, `EXEC`, `DO` blocks, dynamic SQL, stored procedures/functions with unknown behavior, vendor commands, and statements whose effect cannot be established from the supplied SQL as unknown. Do not infer that a `SELECT` is harmless from its leading keyword alone.

For a state-changing or unknown batch, explain before execution: the target datasource and database/schema, the exact statements or a faithful summary, the expected state or permission changes, the likely scope, and that SQLX starts in autocommit without an implicit all-or-nothing transaction. If the user has not explicitly authorized that operation and scope, pause and ask for confirmation. A request to inspect, explain, draft, or find a problem does not authorize a write. A direct request to execute a specific statement against a named target is explicit authorization; do not ask the same confirmation again, but still state the side effect before running it.

Establish the blast radius with read-only SQL before asking: inspect the schema with the recipe for the connected database, and reach the affected rows through the same predicate with `SELECT COUNT(*)` or `EXISTS`. Report those findings instead of describing the change as small. A statement without a limiting predicate reaches the whole table, and a committed batch cannot be undone through this CLI: `DROP`, `TRUNCATE`, and a data-discarding `ALTER` are irreversible, so name the specific operation instead of approving "a change".

Complete this check for the entire batch before executing any statement. Do not run a read-only prefix and then wait before an unapproved write, because earlier statements may already have committed. If the user approves only part of a batch, prepare a separate command and show the changed statement list for confirmation; do not silently rewrite or reorder the original SQL.

The same approval gate applies to `--view`, manual result Refresh, and automatic refresh. A one-time approval does not authorize future reruns of a state-changing or unknown batch. Keep automatic refresh disabled unless the user explicitly approves repeated execution, its interval, and its side effects. Reloading, pagination, reconnecting, and returning to a page read cached results and do not authorize a new SQL execution.

Read [execution and results](execution.md) for how a partial or unknown outcome is represented, and report it as that file describes.
