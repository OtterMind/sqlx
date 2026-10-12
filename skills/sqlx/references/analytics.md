# Charts and dashboards

A chart owns the read-only SQL it runs, the datasource it runs against and the snapshot it renders. There is no separate saved-query object: creating a chart is the whole definition, and the browser editor edits the same chart. Use these commands when the user wants a chart, a dashboard or a picture of query results without asking them to open the workbench.

```sh
sqlx analytics list
sqlx analytics dashboard add --name "Sales" --description "Monthly revenue by region"
sqlx analytics chart add --name "Monthly revenue" --datasource dev \
  --command "SELECT month, SUM(revenue) AS revenue FROM sales GROUP BY month" \
  --type line --x month --y revenue --run --dashboard "Sales"
sqlx analytics chart run "Monthly revenue"
sqlx analytics remove chart "Monthly revenue"
```

- `--name`, `--datasource` and at least one `--command` are required (`--sql` is accepted as an alias, exactly as in `sqlx sql execute`). `--datasource` and every id-taking command accept a name or an id; an ambiguous name is an error, so ask the user or pass the id.
- `--command` may repeat; the editor edits the first statement, and `chart run` executes all of them exactly as `sqlx sql execute` would.
- `--run` executes the SQL immediately and publishes the snapshot the chart renders. Without it the chart is saved as a draft with no snapshot; the workbench shows it as pending until the user runs it.
- `--dashboard <name|id>` places the new chart at the bottom of that dashboard; `--dashboard` also accepts a dashboard that already exists.
- `--type` accepts `column`, `bar`, `line`, `arealine`, `scatter`, `pie`, `ringpie`, `rosepie`, `funnel`, `wordcloud`, `statistics`, `combo` or `table`.
- `--x` names the column plotted along the X axis and `--y` the numeric column plotted on the Y axis (repeat `--y` for several series). A table needs neither, a statistic card needs only `--y`, and every other type needs both. Offer only columns that the chart's SQL returns; run the SQL once (for example with `sqlx sql execute`) when the user has not decided yet.
- Both `--x` and `--y` are validated against the published snapshot: a column the SQL does not return, a duplicate binding or a non-numeric metric is rejected with a message naming the problem. Left/right axes, grouping and the line style are editor-only settings; the CLI writes the single-axis, ungrouped form.

## What the commands guarantee

- SQL passes the same read-only check as `sqlx sql execute`: a write, `SELECT INTO`, a locking read or several statements in one `--command` are rejected before anything is stored.
- `chart run` never replaces a snapshot with a failure. A run whose SQL fails keeps the chart's previous snapshot and reports the database error; the snapshot the chart already had stays readable.
- A chart that is already running the same SQL twice is deduplicated per request, and a chart edited between publication attempts is refused instead of overwriting the newer edit.
- `remove chart` detaches the chart from every dashboard first and then deletes it. `remove dashboard` deletes the dashboard together with the charts placed on it.

## Where the data lives

Under the SQLX data directory (`--data-dir`, default `~/.sqlx`):

```text
analytics/catalog.json          # schema_version, charts, dashboards
analytics/catalog.lock          # short cross-process lock
analytics/snapshots/<uuid>/     # immutable rows, metadata and provenance
analytics/pending/              # unpublished executions
```

A snapshot copies the completed rows, seals its metadata and only then moves the catalog pointer, so an interrupted run leaves the previous snapshot in place. Each chart keeps its current snapshot plus the three preceding ones; anything older and unreferenced is collected. The 24-hour result cache used by `sqlx sql execute` is a different store.

The workbench (`sqlx ui`) renders these charts on `/dashboards`. Creating them does not need the service, so `sqlx analytics` is the path to use when the user cannot open a browser.
