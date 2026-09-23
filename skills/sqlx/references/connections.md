# Connections

A datasource is a saved, encrypted connection. Every command below accepts `--id <id>` or the unique datasource name, for example `sqlx datasource show --id dev`.

## Find and inspect

```sh
sqlx init
sqlx datasource list
sqlx datasource show --id dev
sqlx datasource test --id dev
```

`init` creates the local key and device identity; it does not upload device information. `show` prints the non-secret configuration. Required workers download automatically, so the first `test` or query can wait for a download; see [downloads](downloads.md).

## Create a connection

Let the user type the password in a local page:

```sh
sqlx datasource add --ui --name dev --type postgresql --host db.example.com --port 5432 --database app
```

Without a browser, name the environment variables that hold the credentials:

```sh
sqlx datasource add --name dev --type postgresql --host db.example.com --port 5432 --database app \
  --username-env DB_USER --password-env DB_PASSWORD
```

`--username-env` and `--password-env` take variable names, never literal secrets, and are read at execution time. Never ask for a password in the conversation, and never put credentials in command arguments or logs. Read [local pages](local-ui.md) for the browser flow and its status polling.

For structured non-interactive input, `--connection-stdin` reads one connection object from stdin:

```json
{
  "database_type": "postgresql",
  "host": "localhost",
  "port": 5432,
  "database": "app",
  "service": "",
  "username": "example_account",
  "password": "",
  "tls": "verify-full",
  "properties": {}
}
```

- `database_type` accepts `mysql`, `mariadb`, `tidb`, `postgresql` (`postgres`/`pgsql`), `cockroachdb` (`cockroach`/`crdb`), `yugabytedb` (`yugabyte`/`yb`), `oracle`, `sqlserver` (`mssql`), `clickhouse`, `trino`, `starrocks` and `doris`.
- `tls` is `verify-full` (the default) or `disable`.
- `service` is the Oracle service name; other engines leave it empty.
- `properties` carries optional vendor connection properties, for example a driver timeout.
- `password` may be empty when the engine accepts one, such as a local test fixture, or when the credentials come from environment variables at execution time.

## Change or remove a connection

`sqlx datasource update --id dev` changes the saved settings, or add `--ui` to edit them in the browser. `sqlx datasource remove --id dev` deletes it. Changing the database type requires a new datasource.

## Import connections in bulk

When the user already has connections in another tool, `sqlx datasource import --stdin` (SQLX 0.1.16 or newer) accepts one document instead of asking for every connection again:

```json
{
  "version": 1,
  "mode": "merge",
  "datasources": [
    { "name": "dev", "connection": { "database_type": "postgresql", "host": "localhost", "port": 5432, "database": "app", "username": "example_account", "password": "replace_with_real_input", "tls": "verify-full" } }
  ]
}
```

Each `connection` is the same object `--connection-stdin` accepts, and the password belongs in the document rather than in an argument. `merge` adds new names and updates existing ones in place. An entry with an unknown engine or incomplete parameters is reported in `skipped` with a reason instead of failing the import, `--dry-run` reports without storing, `--strict` refuses the whole document when any entry is invalid, and `--file <path>` reads the document from a file. Read the report and tell the user which entries were skipped instead of assuming every entry was imported.

## Engine-specific fields

- Oracle uses `--service <service-name>` instead of a database name.
- Trino requires `--database <catalog>[.<schema>]` and a username; a password is only sent when TLS is enabled.
- ClickHouse connects to its HTTP port, 8123 by default.
- TiDB defaults to port 4000, StarRocks and Doris to 9030, and YugabyteDB to 5433.
- TiDB, StarRocks and Apache Doris reuse the MySQL worker, and CockroachDB and YugabyteDB reuse the PostgreSQL worker, so a `mysql` or `postgresql` datasource reaches the same server.

Write SQL for the connected engine with its recipe; read [approval](approval.md) before a statement that is not clearly read-only.

## TLS

TLS defaults to certificate verification. Use `--tls disable` only when that connection is explicitly intended to be unencrypted, such as an isolated test fixture. A container database usually does not serve a certificate the machine trusts, and `verify-full` then fails with an unknown-issuer, handshake, or closed-connection error.
