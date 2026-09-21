# MariaDB, CockroachDB, ClickHouse and Trino

These engines are supported in addition to MySQL, PostgreSQL, Oracle and SQL Server. Each one has its own connection defaults.

| Type | Engine | Default port | Target argument | Notes |
| --- | --- | --- | --- | --- |
| `mariadb` | MariaDB | 3306 | `--database <name>` | Speaks the MySQL protocol through the same native worker, so a `mysql` datasource also works against MariaDB. |
| `cockroachdb` (`cockroach`, `crdb`) | CockroachDB | 26257 | `--database <name>` | Speaks the PostgreSQL protocol through the same native worker, so a `postgresql` datasource also works against CockroachDB. |
| `clickhouse` | ClickHouse | 8123 | `--database <name>`, `default` when omitted | Uses the official JDBC driver and must reach the HTTP port, not the native 9000 port. The driver refuses a result set whose columns share a label, so a query that returns duplicate aliases fails on ClickHouse; every other engine keeps duplicate labels distinct. |
| `trino` | Trino | 8080 | `--database <catalog>[.<schema>]` | Uses the official JDBC driver. A user is required. Trino accepts a password only over TLS, so a plaintext connection passes the user alone. |

## Connection examples

```sh
sqlx datasource add --name mariadb --type mariadb --host 127.0.0.1 --port 3306 --database app \
  --tls disable --username-env DB_USER --password-env DB_PASSWORD
sqlx datasource add --name crdb --type cockroachdb --host 127.0.0.1 --port 26257 --database defaultdb \
  --tls disable --username-env DB_USER --password-env DB_PASSWORD
sqlx datasource add --name analytics --type clickhouse --host 127.0.0.1 --port 8123 --database default \
  --tls disable --username-env CH_USER --password-env CH_PASSWORD
sqlx datasource add --name warehouse --type trino --host 127.0.0.1 --port 8080 --database tpch.tiny \
  --tls disable --username-env TRINO_USER
```

## Downloads

`sqlx prefetch mariadb` fetches the MySQL worker, `sqlx prefetch cockroachdb` the PostgreSQL worker, and `sqlx prefetch clickhouse` or `sqlx prefetch trino` the JDBC runner, the pinned JRE and the engine's driver jars. `sqlx prefetch all` covers everything.

## SQL differences worth remembering

- Each `--sql` argument is one statement for every engine. ClickHouse does not accept multiple statements in one request, and Trino rejects writes against the read-only `tpch`, `tpcds`, `jmx` and `system` catalogs.
- CockroachDB lowercases unquoted identifiers, like PostgreSQL; MariaDB returns alias case as written, like MySQL.
- ClickHouse column types such as `Int64` and `Decimal(18, 4)` are reported as the driver names them; values keep their exact text.
