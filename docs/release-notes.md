SQLX 0.1.19 turns the local companion into a small analytics workspace: charts and dashboards built from your own read-only SQL, created from the CLI or from the page.

## Charts and dashboards

- A chart owns its SQL, the datasource it runs against and the snapshot it renders. **+ 添加图表** in the browser opens an editor with three views — chart, result set and SQL — beside a name, a type and only the bindings that type uses: a table lists every column and needs neither axis, a statistic card needs only a value, every other type needs an X and a Y. Both axes list the columns of the chart's result set, so the choices follow the SQL.
- Running the SQL publishes a new snapshot and reloads all three views; the previous snapshot stays available as history. A run that fails keeps the snapshot the chart already showed.
- Thirteen chart types render from immutable snapshots: Column, Bar, Line, AreaLine, Scatter, Pie, RingPie, RosePie, Funnel, WordCloud, Statistics, Combo and Table. Exact decimal strings, NULL and empty strings stay distinct all the way to the screen.
- Dashboards are a 12-column grid. Cards drag by their grip and resize from the corner, layout changes save on their own, and presentation mode is read-only. Deleting a dashboard deletes its charts.

## Create them without the browser

```sh
sqlx analytics chart add --name "Monthly revenue" --datasource "Sales" \
  --sql "SELECT month, SUM(revenue) AS revenue FROM sales GROUP BY month" \
  --type line --x month --y revenue --run --dashboard "Sales board"
sqlx analytics chart run "Monthly revenue"
sqlx analytics list
sqlx analytics remove chart "Monthly revenue"
```

Every command answers with the same JSON envelope as the rest of the CLI, and names resolve by name or id.

## Query history

- `sqlx sql execute --view --description "Row count" …` labels a stored result. The history list and the result page show that label instead of the datasource name, so several queries against one database stay apart.
- The result set table — numbered pages, a rows-per-page picker with any size, click-to-inspect cells — is one component shared by the result page, the chart editor and Table charts.

## Interface

- English and Simplified Chinese are both complete, with a language switcher in the top bar; the choice is remembered.
- The default UI plugin ships with the release and shares its version. Rebuilt assets carry a content stamp, so a redeployed interface is never served from a stale cache.

## Defaults

- Datasources connect **without TLS** unless you ask for it: `--tls verify-full` verifies the server certificate, and a plain local or internal database connects on the first try.
- Result paging accepts any page size; a page ends at the row count or near 2 MiB, whichever comes first.

## Upgrading

Run `sqlx update check` and `sqlx update install`. Updating preserves running UI services, saved datasources, stored results and the selected plugin. The 0.1.19 Skill is published with a `>=0.1.19, <0.2.0` CLI requirement, so update the CLI before the Skill.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.18...v0.1.19
