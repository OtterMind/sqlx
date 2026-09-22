# Execution and results

## Execution model

```text
sqlx sql execute --datasource <id> --command "SELECT 1" --command "SELECT 2"
```

Each `--command` is a complete driver statement. A call uses one connection and executes statements in order. The initial mode is autocommit, and the first error stops the remaining statements. A batch is not automatically atomic. There is no SQL-file input, client-script interpreter, or cross-call session. Put operations requiring a temporary table or session variable in the same call. Do not submit `GO`, `DELIMITER`, or psql backslash commands as SQL.

`--command` is the current flag and `--sql` is still accepted as an alias.

Before running a statement that changes state, read [approval](approval.md).

## Output

Output is a complete JSON object with an ordered `events` array. `columns` describes a result, `row` carries positional values, and `result_end` contains row/update counts. Duplicate column names are preserved. Numeric values use strings to retain precision; binary values use Base64. Some PostgreSQL types without a text decoder retain their wire value as Base64 with the database type. Use explicit SQL casts when human-readable text is preferable.

Check both the top-level `success` and process exit status. `error` and `skipped` events identify partial progress. An incomplete response, timeout, or `outcome: unknown` does not prove a write failed; inspect database state before deciding whether to retry. Do not replay the entire batch automatically. Prior successful statements may already have committed.

The CLI does not truncate results. Limit exploratory queries in SQL to avoid overflowing the agent's own output budget. Do not assume an agent tool showing only part of stdout means the database returned only those rows.

## Report the outcome

After a state-changing execution, report the actual target, statement outcome, affected-row or result information provided by the driver, and any partial or unknown outcome. Offer a focused read-only verification when useful, but do not issue an unrequested compensating write or claim that a failed client response rolled back database changes.
