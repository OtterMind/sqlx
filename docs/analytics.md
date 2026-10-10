# Local analytics

The default UI provides `/dashboards` and `/dashboard/:id`. A chart owns the read-only
SQL it runs, the datasource it runs against and the snapshot it renders; there is no
separate saved-query layer. The chart editor (**编辑图表** on a card, or **+ 添加图表**)
is three views — the chart, the result set and the SQL — beside a short configuration
of a name, a type and only the bindings that type uses: a table lists every column and
needs neither axis, a statistic card needs only a value, every other type needs an X
and a Y. Both axes list the columns of the chart's current result set, so the choices
follow the SQL. Writing SQL and pressing **运行查询** stores a new snapshot and reloads
all three views; the previous snapshot stays in the chart's history. The result page
displays rows only. Older `/datasets` and `/dataset/:id` links redirect to
`/dashboards`. Use a read-only database account; chart SQL is parsed to reject writes,
SELECT INTO, locking and multiple statements in one input. SQL functions can have
database-specific side effects, so the syntax check does not replace database
permissions.

The same objects are created without the browser:

```sh
sqlx analytics chart add --name "Monthly revenue" --datasource "Sales" \
  --sql "SELECT month, SUM(revenue) AS revenue FROM sales GROUP BY month" \
  --type line --x month --y revenue --run --dashboard "Sales board"
sqlx analytics chart run "Monthly revenue"
sqlx analytics list
sqlx analytics remove chart "Monthly revenue"
```

`--run` publishes the snapshot the chart renders; without it the chart is saved as a
draft. `--x` and `--y` are optional per type, and `--y` repeats for several series.
Names resolve by name or id, and an ambiguous name is an error rather than a guess.

Supported views: Column, Bar, Line, AreaLine, Scatter, Pie, RingPie, RosePie,
Funnel, WordCloud, Statistics, Combo and Table. Lines can be straight, smooth or
stepped. Columns, bars and areas support grouped or stacked series. Combo
supports columns, lines, areas and scatter series with left/right axes. A chart
can bind up to three grouping fields and eight metrics, producing at most 32
series. Each axis requires compatible metric units. Stack keys include the
metric, series type, axis and unit. NULL, empty strings, numeric values and
separator-containing labels retain distinct tuple identities.

Category charts require unique primary-dimension + grouping grain. Aggregate
duplicates in SQL instead of summing them in the browser. Scatter allows
repeated numeric X values. Pie-family, funnel and word cloud use one nonnegative
metric; Statistics requires exactly one summary row and displays the original
exact value. Charts load complete snapshots up to 100,000 rows and explicitly
reject larger inputs instead of silently truncating them. Table paginates these
rows and preserves exact cell values. Graph coordinates use floating-point
numbers; scale values outside JavaScript's safe numerical range in SQL.

Dashboards use a 12-column grid. A card drags only by its grip and resizes from the
corner; every layout change is saved automatically and the header reports the
outcome (a transient `Saved 17:32` or a persistent error with **重试**). The header
carries one primary action (**+ 添加图表**) plus presentation, refresh and an overflow
menu for board settings and deletion. Each card shows a single provenance line —
query · chart type · snapshot time · row count — and reveals edit/remove icons on
hover. Presentation mode is read-only and freezes drag and resize. Narrow screens
stack cards while saving the 12-column layout. SQL parameters and scheduled refresh
are not part of this local version. Opening a page never replays SQL. Manual refresh
runs each distinct live query once, with three concurrent executions at most;
fixed-snapshot charts do not refresh. Failure preserves each query's preceding
successful snapshot, including when a query or chart schema becomes invalid.

## Storage

Under the SQLX data directory:

```text
analytics/
  catalog.json                 # schema_version=1; charts, dashboards
  catalog.lock                 # short cross-process definition/pointer lock
  snapshots/<uuid>/
    metadata.json              # result sets, column types, SQL, execution time
    provenance.json            # chart ID and revision that produced it
    <statement>-<result>.jsonl  # original structured rows
    <statement>-<result>.idx    # fixed-width row offsets
  pending/                     # unpublished executions and copies
```

Saving snapshots copies completed row files, seals their metadata, and only
then atomically updates the catalog pointer. The ordinary 24-hour result cache
remains separate. Chart edits and dashboard edits have independent optimistic
revisions; a data refresh does not invalidate a layout draft. Each chart keeps its
current snapshot plus three preceding ones, and those references keep a snapshot
durable. Garbage collection removes only unreferenced snapshots
older than one hour. A stopped process never automatically replays pending SQL.
Connection credentials stay in SQLX's existing encrypted datasource store.

## Build and run the Docker MySQL demo

```sh
npm --prefix ui ci
npm --prefix ui run build
cargo build -p ottermind-sqlx -p sqlx-ui -p sqlx-driver-mysql
python3 examples/analytics/mysql-demo.py --data-dir /absolute/path/sqlx-demo
```

The script uses the local `mysql` container, refuses an existing demo schema or
catalog, creates an isolated schema and SELECT-only account, and reads credentials
through stdin. It seeds 288 sales rows (12 months × 4 regions × 2 channels × 3
products), five funnel stages and 16 keywords. It creates 18 charts — each with its
own SQL and snapshot — and two dashboards. It returns both URLs and a CSV export;
the companion stays running. To reopen or stop only this demo, use the same
`SQLX_DATA_DIR` and `SQLX_WORKER_DIR` with `target/debug/sqlx ui` or `ui stop`.

Validation:

```sh
npm --prefix ui test
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 tests/analytics_api.py --source-dir /absolute/path/sqlx-demo
```

The API test copies the fixture's encrypted connection into a temporary SQLX
workspace with its own service/port, uses read-only queries, tests publication,
conflicts, pins, failed refresh and restart persistence, and cleans up that
workspace. It does not alter the MySQL fixture or stop the demo service.
