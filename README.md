# SQLX

A Rust database CLI for agents, with encrypted user-level datasources and independently downloaded database workers.

| Database | Worker |
|---|---|
| MySQL | Native Rust (`mysql_async`) |
| PostgreSQL | Native Rust (`tokio-postgres`) |
| Oracle | Official JDBC Thin driver through a Java worker |
| SQL Server | Official Microsoft JDBC driver through a Java worker |

The initial implementation is available as source. No binary release is published yet. CI validates the five target platforms and runs isolated database integration tests; consult the actual workflow results before treating a target as verified.

## Commands

| Operation | Command |
|---|---|
| Initialize local storage | `sqlx init` |
| Create a datasource | `sqlx datasource add --name dev --type mysql --host localhost --database app --username-env DB_USER --password-env DB_PASSWORD` |
| List connections | `sqlx datasource list` |
| Inspect a connection | `sqlx datasource show --id dev` |
| Change connection settings | `sqlx datasource update --id dev --host db.example.com` |
| Remove a saved connection | `sqlx datasource remove --id dev` |
| Test connectivity | `sqlx datasource test --id dev` |
| Execute SQL | `sqlx sql execute --datasource dev --sql "SELECT 1" --sql "SELECT 2"` |
| Install the Skill | `sqlx skill install --target codex` or `--target claude` |
| Install to another skill directory | `sqlx skill install --path /path/to/skills/sqlx` |
| Inspect/update managed Skills | `sqlx skill status`, `sqlx skill update` |
| Help/version | `sqlx --help`, `sqlx --version` |

`--id` and `--datasource` accept a stable datasource UUID or its unique name. Supported database type names are `mysql`, `postgresql` (`postgres`/`pgsql`), `oracle`, and `sqlserver` (`mssql`). Oracle requires `--service`. TLS defaults to certificate verification; `--tls disable` is available for explicitly unencrypted connections. The first authentication profile is username/password.

Credentials are read from named environment variables, hidden interactive prompts, or a connection JSON object on stdin. Do not put literal passwords in command arguments. For noninteractive creation or complete replacement, `--connection-stdin` accepts:

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

The object goes to stdin of `sqlx datasource add --name dev --connection-stdin`. It is not a SQL file input. Datasource responses omit usernames, passwords, and vendor properties.

## Execution behavior

Each invocation owns one database connection. Repeated `--sql` arguments execute in order, initially with autocommit, and stop at the first error. There is no implicit all-or-nothing transaction. Temporary tables and session variables do not survive another invocation. Do not submit client directives such as `GO`, `DELIMITER`, or psql backslash commands.

SQL output is one JSON object containing `protocol_version`, `datasource_id`, an ordered `events` array, and `success`. Events distinguish columns, positional row values, result boundaries, statement completion, errors, skipped statements, and overall completion. Duplicate labels remain distinct. Numbers are encoded as strings to preserve integer and decimal precision. Binary data and PostgreSQL types without a text decoder use Base64 with type metadata; an explicit SQL cast to text is available when a readable database representation is preferable.

Rows are streamed without a CLI row limit or silent truncation. The agent's own tool output limits still apply. Check the final success flag and exit status. An incomplete response or unknown write outcome must not trigger automatic replay.

## Installation and resources

Release targets are macOS ARM64/x64, Windows x64, and Linux ARM64/x64. The main binary has no database-driver dependencies. First connection downloads the required worker; JDBC connections also obtain a compatible private JRE and official driver. Skill content is a separate cross-platform package.

Resources are selected from a versioned GitHub Release manifest and verified using SHA-256. Use `--manifest <https-url>` for a selected release or a local test server. Installed components are reused, and their recorded file hashes are checked before execution. `sqlx skill update` refreshes the manifest and updates compatible managed installations without overwriting local edits.

User data defaults to `~/.sqlx/`; `--data-dir` or `SQLX_DATA_DIR` selects another directory. Datasources use AES-256-GCM with a separately generated local key and owner-restricted file permissions. Keep the key with encrypted data when backing up. Possession of both allows decryption. Device IDs are derived locally from available hardware/system identifiers, with an installation fallback; keys are never derived from hardware. This version does not upload device information.

The [English Skill](skills/sqlx/SKILL.md) can also be installed first. It explains how an agent can install the CLI before using it. Each [database reference](skills/sqlx/references/) explains every SQL operation separately and links to its official documentation.

## Development and validation

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
mvn -B -f java/jdbc/pom.xml verify
python3 tests/distribution.py
docker compose -f tests/compose.yaml up -d --wait
python3 tests/integration.py
docker compose -f tests/compose.yaml down -v
```

For local native workers, set `SQLX_WORKER_DIR` to the absolute `target/debug` directory. For JDBC development, that directory also contains `sqlx-jdbc.jar` and `ojdbc.jar` or `mssql-jdbc.jar`; `SQLX_JAVA_BIN` can select Java 17 explicitly. These overrides are for development, not prerequisites for release users. The fixture scripts use dedicated test containers and test-only credentials.

See [the design](docs/design.md) for implementation boundaries and deferred features. SQL-file input, persistent sessions, configurable transaction/error modes, result-file export, automatic CLI updates, and telemetry are not v1 features.

## License

This public source repository retains the modified Chat2DB license for adapted code. It is not the unmodified Apache 2.0 license. See [LICENSE](LICENSE) and [NOTICE](NOTICE); third-party dependencies retain their own licenses.
