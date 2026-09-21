# Connections

A datasource is a saved, encrypted connection. Each command below works with `--id <id>` or the unique datasource name.

## Find and inspect

```sh
sqlx init
sqlx datasource list
sqlx datasource show --id <id>
sqlx datasource test --id <id>
```

`init` creates the local key and device identity; it does not upload device information. `show` prints the non-secret configuration. Required workers download automatically, so the first `test` or query can wait for a download; see [downloads](downloads.md).

## Create without a browser

```sh
sqlx datasource add --name dev --type postgresql --host db.example.com --port 5432 --database app \
  --username-env DB_USER --password-env DB_PASSWORD
```

`--username-env` and `--password-env` take variable names, never literal secrets, and are read at execution time. When the user should type the password themselves, use the local page in [local pages](local-ui.md) instead of asking for it in the conversation.

For structured non-interactive input, `--connection-stdin` accepts the connection object documented in the repository README:

```json
{
  "database_type": "postgresql",
  "host": "localhost",
  "port": 5432,
  "database": "app",
  "service": "",
  "username": "example_account",
  "password": "replace_with_real_input",
  "tls": "verify-full",
  "properties": {}
}
```

Do not put credentials in command arguments or logs. `sqlx datasource update --id <id>` changes a saved connection; changing the database type requires a new datasource.

## Engine-specific fields

- Oracle uses `--service <service-name>` instead of a database name.
- Trino requires `--database <catalog>[.<schema>]` and a username; a password is only sent when TLS is enabled.
- ClickHouse connects to its HTTP port, 8123 by default.
- TiDB defaults to port 4000, StarRocks and Doris to 9030, and YugabyteDB to 5433.
- TiDB, StarRocks and Apache Doris reuse the MySQL worker, and CockroachDB and YugabyteDB reuse the PostgreSQL worker, so a `mysql` or `postgresql` datasource reaches the same server.

Write SQL for the connected engine with its recipe; read [approval](approval.md) before a statement that is not clearly read-only.

## TLS

TLS defaults to certificate verification. Use `--tls disable` only when that connection is explicitly intended to be unencrypted, such as an isolated test fixture. A container database usually does not serve a certificate the machine trusts, and `verify-full` then fails with an unknown-issuer, handshake, or closed-connection error.
