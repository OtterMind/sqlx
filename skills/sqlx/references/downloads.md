# Downloads on first use

The CLI does not embed database drivers, the JDBC runtime or the browser UI. The first operation that needs one downloads it from the fixed GitHub Release into the SQLX data directory and reuses it afterwards:

- MySQL and PostgreSQL workers (~5 MB) on the first query for that database type.
- The JDBC runner, the pinned JRE and the vendor driver on the first Oracle or SQL Server query; the JRE is the largest download.
- The UI engine (~6 MB) and the default UI plugin on the first `--view`, `datasource add/update --ui` or `sqlx ui`.

Each download prints `Downloading …`, a progress line with speed and estimated time, and a final `Downloaded … in 12.3s (390 KB/s)` line on stderr. An interrupted transfer is retried up to three times. Tell the user that a first command can wait for a download instead of reporting it as a hang, and re-run the same command after a failure: components that are already installed are reused.

When the network is slow, prefetch ahead of time with `sqlx prefetch <component>`. The accepted component names are `mysql`, `mariadb`, `tidb`, `starrocks`, `doris`, `postgres`, `cockroachdb`, `yugabytedb`, `oracle`, `sqlserver`, `clickhouse`, `trino`, `ui`, `skill` and `all`; `all` includes the JDBC runtime and the JRE. Engines that share a protocol share a worker: `mariadb`, `tidb`, `starrocks` and `doris` fetch the MySQL worker, and `cockroachdb` and `yugabytedb` fetch the PostgreSQL worker. MySQL, PostgreSQL and the local UI come from the archive of the running platform, and the JDBC databases also download the JRE.
