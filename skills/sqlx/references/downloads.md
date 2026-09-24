# Downloads on first use

The CLI does not embed database drivers, the JDBC runtime or the browser UI. The first operation that needs one downloads it from the fixed GitHub Release into the SQLX data directory and reuses it afterwards:

- Engines that speak the MySQL protocol (MySQL, MariaDB, TiDB, StarRocks, Doris) download the MySQL worker, about 5 MB, on the first query.
- Engines that speak the PostgreSQL protocol (PostgreSQL, CockroachDB, YugabyteDB) download the PostgreSQL worker on the first query.
- Oracle, SQL Server, ClickHouse and Trino download the JDBC runner, the pinned JRE and the vendor driver; the JRE is the largest download.
- The UI engine, about 6 MB, and the default UI plugin arrive on the first `--view`, `datasource add/update --ui` or `sqlx ui`.

Each download prints `Downloading …`, a progress line with speed and estimated time, and a final `Downloaded … in 12.3s (390 KB/s)` line on stderr. An interrupted transfer is retried up to three times. Tell the user that a first command can wait for a download instead of reporting it as a hang, and re-run the same command after a failure: components that are already installed are reused.

Run `sqlx prefetch <component>`; the accepted names are the engine names `sqlx prefetch --help` lists, plus `ui`, `skill` and `all`. An engine whose vendor does not allow redistribution, such as `db2`, prefetches the shared Java runtime and the JDBC runner; its driver comes from `sqlx driver add` instead.
