# Execution and results

## Execution model

```text
sqlx sql execute --datasource <id> --command "SELECT 1" --command "SELECT 2"
```

Each `--command` is a complete driver statement. A call uses one connection and executes statements in order. The initial mode is autocommit, and the first error stops the remaining statements. A batch is not automatically atomic. There is no SQL-file input, client-script interpreter, or cross-call session. Put operations requiring a temporary table or session variable in the same call. Do not submit `GO`, `DELIMITER`, or psql backslash commands as SQL.

`--command` is the current flag and `--sql` is still accepted as an alias.

Before running a statement that changes state, read [approval](approval.md).

## Output

Output is one JSON object with **one item per executed statement** under `results`:

```json
{"results":[{"stmt":0,"cols":[["id","int"],["name","varchar"]],
             "rows":[["1","ada"],["2","bob"]],"count":"2"}],"success":true}
```

- `stmt` is the statement index; `set` appears only for a second or later result set of that statement.
- `cols` lists columns as `[name, type]`, and a third entry (`base64`, `boolean` or `json`) appears when values are not plain text. Types are the engine's own names, shortened: `int`, `varchar`, `decimal`, `int4`, `numeric`, `bson`.
- `rows` holds positional values, one array per row; SQL `NULL` is `null`. Duplicate column names are preserved by position. Numbers are strings to keep exact precision, and binary values are Base64 and marked in `cols`.
- `count` is the row count the driver reported for that result set, and `affected` the affected-row count of a write.
- A failed statement carries its own `error` with `code`, `message` and `outcome`; statements that never ran are listed in `skipped`. Read `success` together with the process exit status.

## Large results

Only a preview reaches stdout: at most `preview-rows` rows (10 by default) and 16 KiB per result set. A larger result set is stored completely and its item carries the file instead of the rows:

```json
{"results":[{"stmt":0,"cols":[["id","int"]],"rows":[["1"]],"count":"100005",
             "file":"/var/folders/…/results/7f0c…/0-0.jsonl"}],
 "id":"7f0c…","success":true}
```

When `file` is present, `len(rows)` is smaller than `count` and the remaining rows are in that file: one JSON array per line, in the same encoding as `rows`. Read it directly (`head`, `sed -n '500,520p'`, `jq`) or page it with the stored result:

```sh
sqlx results list
sqlx results rows --id <id> --statement 0 --set 0 --offset 500 --limit 100
```

`sqlx results rows` returns `cols`, `rows`, `offset`, `next_offset`, `total` and `complete`, and never contacts the database again. A result that fits the preview is printed completely and not stored at all. See [results](results.md) for the layout, the settings and the limits.

Never re-run a query only to see more rows: use the stored file or `sqlx results rows`. `--preview <rows>` changes the preview size for one call, `--preview 0` prints no rows, and `--events` prints the raw worker event stream (`protocol_version`, `datasource_id`, `events`, `success`) instead of the table shape and stores nothing.

## Report the outcome

After a state-changing execution, report the actual target, statement outcome, affected-row or result information provided by the driver, and any partial or unknown outcome. An incomplete response, timeout, or `outcome: unknown` does not prove a write failed; inspect database state before deciding whether to retry, and never replay the entire batch automatically: prior successful statements may already have committed. Offer a focused read-only verification when useful, but do not issue an unrequested compensating write or claim that a failed client response rolled back database changes.
