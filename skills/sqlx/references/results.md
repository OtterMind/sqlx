# Stored results and settings

## Where results live

The default result directory is a private directory inside the system temporary directory, named after the user and the data directory, so results disappear when the machine reboots. Every stored result is one directory:

```text
<result-dir>/<result-id>/metadata.json        statements, columns, counts, status, origin
<result-dir>/<result-id>/<stmt>-<set>.jsonl   one JSON array per row, same encoding as `rows`
<result-dir>/<result-id>/<stmt>-<set>.idx     row offsets, used by the pages
```

Store results somewhere persistent when they must survive a reboot:

```sh
sqlx setting set results-dir ~/sqlx-results
sqlx setting set results-retention-hours 0     # 0 keeps them until the size limit removes them
```

Retention defaults to 24 hours, and the stored results of this command line stay under 1 GiB in total; expired and oldest results are removed before the next command runs. `sqlx results list` shows what is stored, including each result's status and row count.

## Reading more rows

```sh
sqlx results rows --id <id> --statement 0 --set 0 --offset 500 --limit 100
```

`--offset` counts rows from the start of that result set, `--limit` accepts 1 to 200 rows (50 by default), and the answer repeats `cols` so it can be read without the original response. `next_offset` is the offset for the following page; `complete` is true when the result set has no more rows. A statement index that was never stored (a result that fit the preview) reports that the result is unknown: those rows were printed in full already, so there is nothing to read back.

Reading a stored result never re-runs SQL. The file itself is plain JSONL, so a shell or file reader is enough: `head -n 20 <file>`, `sed -n '500,520p' <file>`, or `jq -c '.[0]' <file>`.

## Settings

```sh
sqlx setting list                              # every setting with its effective value and source
sqlx setting get preview-rows
sqlx setting set preview-rows 20               # rows printed before a result is stored
sqlx setting set results-dir ~/sqlx-results    # absolute path; `~` is expanded
sqlx setting set results-retention-hours 0     # 0 keeps results until the size limit removes them
sqlx setting unset preview-rows                # back to the default
```

| Setting | Default | Meaning |
|---|---|---|
| `preview-rows` | `10` | Rows printed per result set; more rows are stored and pointed at by `file` |
| `results-dir` | a private directory in the system temporary directory | Where stored results live |
| `results-retention-hours` | `24` | Age after which the next command removes a stored result; `0` disables the age limit |

A command line flag (`--preview`) and the environment (`SQLX_PREVIEW_ROWS`, `SQLX_RESULTS_DIR`, `SQLX_RESULTS_RETENTION_HOURS`) override the file, in that order. Settings live in `<data-dir>/settings.json`, the source column of `sqlx setting list` names what is in effect (`default`, `file`, `env` or `flag`), and an unwritable or relative `results-dir` is rejected instead of failing during a later query.
