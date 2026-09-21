# SQLX

Connect to MySQL, MariaDB, TiDB, PostgreSQL, CockroachDB, YugabyteDB, Oracle, SQL Server, ClickHouse, Trino, StarRocks and Apache Doris from your terminal or from your agent. Connections are saved encrypted, one invocation runs one or more SQL statements, and the results come back complete and structured.

## Quick start

```sh
# 1. Install the CLI (macOS, Linux and Windows x64; Node.js 22 or newer)
npx -y @ottermind/sqlx@latest
export PATH="$HOME/.local/bin:$PATH"

# 2. Create a connection (an interactive terminal prompts for the username and password)
sqlx datasource add --name dev --type postgresql --host db.example.com --port 5432 --database app
sqlx datasource test --id dev

# 3. Run SQL
sqlx sql execute --datasource dev --sql "SELECT current_database()"
```

Using an agent? See [Use with your agent](#use-with-your-agent): install one plugin or extension and the agent calls SQLX directly.

## Install the CLI

Three channels, pick one: prebuilt packages from [GitHub Releases](https://github.com/OtterMind/sqlx/releases), the `npx` installer, or the platform install scripts.

### macOS and Linux

```sh
curl -fsSL https://raw.githubusercontent.com/OtterMind/sqlx/main/scripts/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
sqlx --version
sqlx init
```

The installer selects your platform, downloads the executable and verifies its SHA-256. It installs to `~/.local/bin`; add that directory to your shell's persistent PATH. `SQLX_INSTALL_DIR` chooses another directory and `SQLX_VERSION` selects a release version.

### Windows x64

Run in PowerShell:

```powershell
$installer = Join-Path $env:TEMP 'sqlx-install.ps1'
Invoke-WebRequest 'https://raw.githubusercontent.com/OtterMind/sqlx/main/scripts/install.ps1' -OutFile $installer
powershell -NoProfile -ExecutionPolicy Bypass -File $installer
$env:Path = "$env:LOCALAPPDATA\Programs\SQLX;$env:Path"
sqlx --version
sqlx init
```

Add `%LOCALAPPDATA%\Programs\SQLX` to your user PATH for future sessions. Neither installer overwrites an unrelated executable already named `sqlx`; use a different install directory in that case.

Prebuilt targets are macOS ARM64/x64, Linux ARM64/x64 and Windows x64; the Linux baseline is Ubuntu 24.04. Release users do not need Rust, Java or database drivers: database workers and a private JRE are downloaded only when needed.

### Node.js (`npx`)

macOS, Linux and Windows x64 with Node.js 22 or newer:

```sh
npx -y @ottermind/sqlx@latest
export PATH="$HOME/.local/bin:$PATH"
sqlx --version
```

The installer verifies the release manifest, `SHA256SUMS` and the downloaded archive before installing. It uses the same user-level location as the platform installers and installs the Skill into `./sqlx`; `--target codex`, `--target claude`, `--target dsh`, `--target pi` or `--target <directory>` place the Skill somewhere else. Node.js is needed only by the installer, never by the CLI.

### Updates

The CLI updates its own executable:

```sh
sqlx update check       # report the latest stable version without installing it
sqlx update install     # download, verify and replace the executable
sqlx update status      # read local history without network access
```

`sqlx update install --version <version>` installs an exact stable version. Updates do not stop running SQL or the UI service, do not modify saved connections, and do not update Skills or plugins automatically; use `sqlx skill update` for managed Skills, and UI plugins keep the version you selected.

Interactive use checks in the background at most once per day and only prints a notice on stderr. Piped and CI commands skip that check, and `SQLX_NO_UPDATE_CHECK=1` disables it. Source builds and package-manager-owned paths keep their own installation method.

## Use with your agent

All four are ready as soon as they are installed: the plugin or extension installs the `sqlx` CLI itself on first use, so you never install it separately.

### Codex

```sh
codex plugin marketplace add OtterMind/sqlx@plugins
codex plugin add sqlx@ottermind
```

The plugin starts `sqlx mcp` over MCP. Read-only operations run directly; the two execution tools (`sqlx_sql_execute` and `sqlx_sql_view`) are marked destructive and Codex asks for approval by default.

### Claude Code

```sh
claude plugin marketplace add OtterMind/sqlx@plugins
claude plugin install sqlx@ottermind
```

Headless runs need an explicit tool allowlist:

```sh
claude --allowedTools "mcp__plugin_sqlx_sqlx__*" -p "List my SQLX datasources"
```

### DeepSeek Harness

```sh
dsh plugin --profile web add @ottermind/sqlx-dsh    # browser UI
dsh plugin --profile tui add @ottermind/sqlx-dsh    # terminal UI
```

Plugins belong to a profile: install it into the profile you use, then restart dsh.

### Pi

```sh
pi install npm:@ottermind/sqlx-pi
```

### Skill only

```sh
sqlx skill install --target codex     # Codex and dsh share ~/.agents/skills
sqlx skill install --target claude
sqlx skill install --target dsh
sqlx skill install --target pi

sqlx skill status                     # list managed installations
sqlx skill update                     # update them, keeping local edits
```

For another agent, pass its skill directory with `sqlx skill install --path <directory>`.

## Connections and SQL

### Create a connection

```sh
sqlx datasource add --name dev --type postgresql --host db.example.com --port 5432 --database app
sqlx datasource test --id dev
sqlx sql execute --datasource dev --sql "SELECT current_database()" --sql "SELECT 1"
```

| Database | `--type` values | Execution | Required values and defaults |
|---|---|---|---|
| MySQL | `mysql` | native worker | port 3306 by default |
| MariaDB | `mariadb` | reuses the MySQL worker | port 3306 by default |
| TiDB | `tidb` | reuses the MySQL worker | port 4000 by default |
| StarRocks | `starrocks` | reuses the MySQL worker | port 9030 by default; has no user database until one is created |
| Apache Doris | `doris` | reuses the MySQL worker | port 9030 by default; has no user database until one is created |
| PostgreSQL | `postgresql`, `postgres`, `pgsql` | native worker | port 5432 by default |
| CockroachDB | `cockroachdb`, `cockroach`, `crdb` | reuses the PostgreSQL worker | |
| YugabyteDB | `yugabytedb`, `yugabyte`, `yb` | reuses the PostgreSQL worker | port 5433 by default |
| Oracle | `oracle` | JDBC worker | `--service <service-name>` is required |
| SQL Server | `sqlserver`, `mssql` | JDBC worker | port 1433 by default |
| ClickHouse | `clickhouse` | JDBC worker | connects to the HTTP port, 8123 by default |
| Trino | `trino` | JDBC worker | `--database <catalog>[.<schema>]` is required |

`--id` and `--datasource` accept a stable datasource UUID or its unique name.

To let the user type the password in a local page, so it never enters the conversation or a tool response:

```sh
sqlx datasource add --ui --name dev --type postgresql --host db.example.com --port 5432 --database app
sqlx datasource setup-status --request-id <request-id>
```

To edit an existing connection and keep its saved password unless you replace it: `sqlx datasource update --id dev --ui`.

TLS verifies the database certificate by default. A database in a local container usually does not serve a certificate your machine trusts, and then the connection fails with `invalid peer certificate: UnknownIssuer` (MySQL), `error performing TLS handshake` (PostgreSQL) or a closed connection (Oracle). Disable the transport explicitly for such a connection:

```sh
sqlx datasource add --name dev --type mysql --host 127.0.0.1 --port 3306 --database app \
  --username-env DB_USER --password-env DB_PASSWORD --tls disable
sqlx datasource update --id dev --tls disable
```

### Credentials

Credentials come from named environment variables, hidden interactive prompts, or a connection JSON object on stdin. Never put a literal password in a command argument.

```sh
sqlx datasource add --name dev --connection-stdin
```

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

Datasource responses omit usernames, passwords and vendor properties.

### Commands

| Operation | Command |
|---|---|
| Initialize local storage | `sqlx init` |
| Create a connection | `sqlx datasource add --name dev --type mysql --host localhost --database app --username-env DB_USER --password-env DB_PASSWORD` |
| List connections | `sqlx datasource list` |
| Inspect a connection | `sqlx datasource show --id dev` |
| Change connection settings | `sqlx datasource update --id dev --host db.example.com` |
| Remove a saved connection | `sqlx datasource remove --id dev` |
| Test connectivity | `sqlx datasource test --id dev` |
| Execute SQL | `sqlx sql execute --datasource dev --sql "SELECT 1" --sql "SELECT 2"` |
| Download workers, the JDBC runtime and the UI ahead of time | `sqlx prefetch mysql ui` (`mariadb`, `tidb`, `starrocks`, `doris`, `postgres`, `cockroachdb`, `yugabytedb`, `oracle`, `sqlserver`, `clickhouse`, `trino`, `skill` or `all`) |
| Execute and open a result page | `sqlx sql execute --datasource dev --sql "SELECT 1" --view` |
| Local workbench | `sqlx ui`, `sqlx ui status`, `sqlx ui stop` |
| Serve MCP over stdio | `sqlx mcp` |
| Install the Skill | `sqlx skill install --target codex`, `--target claude`, `--target dsh` or `--target pi` |
| Install to another skill directory | `sqlx skill install --path /path/to/skills/sqlx` |
| Inspect and update managed Skills | `sqlx skill status`, `sqlx skill update` |
| Stop managing a Skill installation | `sqlx skill remove --path /path/to/skills/sqlx` (files are kept) |
| Help and version | `sqlx --help`, `sqlx --version` |

### Execution behavior

Each invocation owns one database connection. Repeated `--sql` arguments execute in order, initially with autocommit, and stop at the first error; there is no implicit all-or-nothing transaction. Temporary tables and session variables do not survive another invocation. Do not submit client directives such as `GO`, `DELIMITER` or psql backslash commands.

SQL output is one JSON object containing `protocol_version`, `datasource_id`, an ordered `events` array and `success`. Events distinguish columns, positional row values, result boundaries, statement completion, errors, skipped statements and overall completion; duplicate labels remain distinct. Numbers are encoded as strings to preserve integer and decimal precision; binary data and PostgreSQL types without a text decoder use Base64 with type metadata, and an explicit SQL cast to text is available when a readable database representation is preferable.

Rows are streamed without a CLI row limit or silent truncation; the agent's own tool output limits still apply. Check the final `success` flag and the exit status, and never replay an uncertain write automatically.

## Local pages

Let the user inspect results in a browser and enter the password there.

```sh
sqlx sql execute --datasource dev --sql "SELECT id, name FROM users ORDER BY id" --view
```

SQLX executes once and returns a local URL. The page loads the results automatically, supports multiple result sets and pagination, and preserves exact values. Reloading, paging or reopening the page reads the cached result; **Refresh** on the page reruns the original SQL batch against the database, so any writes in that batch run again. A successful refresh replaces the displayed snapshot at the same URL; a failure keeps the previous result and does not roll back database changes. Results are retained locally for 24 hours.

`sqlx ui` opens the local workbench, `sqlx ui status` and `sqlx ui stop` inspect or stop the service, and `--no-open` returns a link without launching a browser. Pages are reachable only on the machine running SQLX, load an HttpOnly browser session automatically, and the service stays up until `sqlx ui stop`.

Typing the password into the page keeps credentials out of the conversation, but it does not isolate them from an agent that can read files or control the browser as the same operating-system user. See [the local UI design](docs/local-ui.md) for the interface and storage boundaries.

### Choose your UI

```sh
sqlx ui plugin install --url <plugin-zip-url> --sha256 <published-sha256>
sqlx ui plugin list
sqlx ui plugin use <plugin-id>          # default restores the default interface
sqlx ui plugin remove <plugin-id> --version <version>
```

Installing does not activate a plugin; after selecting it, reload an open page or run `sqlx ui`. CLI updates keep the UI plugin version you selected, and switching versions needs `sqlx ui stop` first. Plugins run locally and can access entered credentials and displayed data, so install interfaces from authors you trust: a checksum proves the downloaded bytes are intact, not that the author is trustworthy.

To build your own interface, see the [UI plugin guide](docs/ui-plugins.md), the [typed browser SDK](ui/sdk/client.ts) and the independent [terminal UI example](examples/terminal-ui/). Users need no Node.js runtime.

## Data and downloads

User data lives in `~/.sqlx/`; use `--data-dir` or `SQLX_DATA_DIR` for another location. Saved connections use AES-256-GCM with an independently generated local key: back up the key together with the encrypted data, because losing the key prevents decryption. Device identity is generated locally and this version uploads no device information.

The main executable contains no database drivers; each database's worker is downloaded on first use (the [database table](#create-a-connection) lists which worker serves which database). Downloaded resources come from the fixed release manifest of the running CLI version and are verified before use; `--manifest <https-url>` selects another manifest or a local test server.

Downloads happen on first use and are cached afterwards. Each one prints `Downloading …` with speed and estimated time, and a final `Downloaded … in 12.3s (390 KB/s)` line on stderr; the progress line is refreshed only when stderr is a terminal, so piped JSON stays clean. An interrupted transfer is retried up to three times, and rerunning a failed command reuses every component that is already installed. To avoid waiting inside the first query or page:

```sh
sqlx prefetch mysql ui      # MySQL worker and the local browser UI
sqlx prefetch all           # adds the PostgreSQL, CockroachDB, YugabyteDB, MariaDB, TiDB, StarRocks, Doris, Oracle, SQL Server, ClickHouse and Trino workers, the JDBC runtime and the JRE
```

The [database references](skills/sqlx/references/) explain each SQL operation's purpose, parameters, result and official documentation link.

## Troubleshooting

| Symptom | What to do |
|---|---|
| `invalid peer certificate: UnknownIssuer`, `error performing TLS handshake` | The database serves no certificate your machine trusts (common in local containers); add `--tls disable` to that connection |
| `sqlx: command not found` | The install directory is missing from PATH: `~/.local/bin` (macOS, Linux) or `%LOCALAPPDATA%\Programs\SQLX` (Windows) |
| The first query seems stuck downloading | Run `sqlx prefetch <component>` first; after an interruption, rerunning reuses installed components |
| Switching the UI plugin version fails | Run `sqlx ui stop` first, then `sqlx ui plugin remove` |
| An update fails | Check `sqlx update status`; rerun the installer if needed, saved connections are unaffected |
| Oracle or SQL Server reports a JDBC-related error | Rerun with `SQLX_JDBC_DEBUG=1` to see the driver's own diagnostics |

## Build from source

Source development requires Git, Rust 1.95, Node.js 22 and the platform's native build tools; Node.js only builds UI plugin assets. For Oracle or SQL Server development, also install a Java 17 JDK and Maven.

On macOS or Linux:

```sh
git clone https://github.com/OtterMind/sqlx.git
cd sqlx
npm --prefix ui ci
npm --prefix ui run build
cargo build --workspace --release --locked
export PATH="$PWD/target/release:$PATH"
export SQLX_WORKER_DIR="$PWD/target/release"
sqlx --version
sqlx init
sqlx ui plugin install --path ui/dist
sqlx ui plugin use default
```

On Windows PowerShell:

```powershell
git clone https://github.com/OtterMind/sqlx.git
Set-Location sqlx
npm --prefix ui ci
npm --prefix ui run build
cargo build --workspace --release --locked
$env:Path = "$PWD\target\release;$env:Path"
$env:SQLX_WORKER_DIR = "$PWD\target\release"
sqlx --version
sqlx init
sqlx ui plugin install --path ui/dist
sqlx ui plugin use default
```

Keep the checkout at that location, or copy the CLI and both native workers into a dedicated directory and update PATH and `SQLX_WORKER_DIR` accordingly. This source build makes MySQL and PostgreSQL usable without a published worker manifest.

For Oracle and SQL Server, build the JDBC worker and place its driver JARs alongside those workers. On macOS or Linux:

```sh
mvn -B -f java/jdbc/pom.xml package
cp java/jdbc/target/sqlx-jdbc-0.1.11.jar target/release/sqlx-jdbc.jar
curl -fL https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar -o target/release/ojdbc.jar
curl -fL https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar -o target/release/mssql-jdbc.jar
```

On Windows PowerShell:

```powershell
mvn -B -f java/jdbc/pom.xml package
Copy-Item java/jdbc/target/sqlx-jdbc-0.1.11.jar target/release/sqlx-jdbc.jar
Invoke-WebRequest 'https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar' -OutFile target/release/ojdbc.jar
Invoke-WebRequest 'https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar' -OutFile target/release/mssql-jdbc.jar
```

Java 17 must be on PATH, or `SQLX_JAVA_BIN` can point to its executable. These manual dependencies are only needed for source development; a binary release downloads its private Java runtime and drivers automatically.

## Development and validation

```sh
npm --prefix ui ci
npm --prefix ui run build
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
mvn -B -f java/jdbc/pom.xml verify
python3 tests/distribution.py
python3 tests/ui_lifecycle.py
python3 tests/ui_distribution.py
python3 tests/ui_plugins.py
python3 tests/updates.py
docker compose -f tests/compose.yaml up -d --wait mysql postgres
python3 tests/integration.py
python3 tests/ui_api.py
docker compose -f tests/compose.yaml down -v
# one group at a time; StarRocks and Doris each need a frontend and a backend
docker compose -f tests/compose.yaml up -d --wait tidb yugabytedb
python3 tests/databases.py tidb yugabytedb
docker compose -f tests/compose.yaml down -v
docker compose -f tests/compose.yaml up -d --wait starrocks
python3 tests/databases.py starrocks
docker compose -f tests/compose.yaml down -v
docker compose -f tests/compose.yaml up -d --wait doris
python3 tests/databases.py doris
docker compose -f tests/compose.yaml down -v
```

For local native workers, set `SQLX_WORKER_DIR` to the absolute `target/debug` directory. For JDBC development, that directory also contains `sqlx-jdbc.jar` and `ojdbc.jar` or `mssql-jdbc.jar`; `SQLX_JAVA_BIN` can select Java 17 explicitly. These overrides are for development, not prerequisites for release users. The fixture scripts use dedicated test containers and test-only credentials.

## Releases and documentation

- Packages and checksums: [GitHub Releases](https://github.com/OtterMind/sqlx/releases)
- Release notes: [docs/release-notes.md](docs/release-notes.md)
- Design and implementation boundaries: [docs/design.md](docs/design.md)
- Update behavior and recovery: [docs/updates.md](docs/updates.md)
- Local interface and plugins: [docs/local-ui.md](docs/local-ui.md), [docs/ui-plugins.md](docs/ui-plugins.md)
- Agent integrations: [integrations/README.md](integrations/README.md), [Skill](skills/sqlx/SKILL.md)

Not included: SQL-file input, persistent sessions, configurable transaction and error modes, result-file export, unattended update installation, and telemetry.

## License

This public source repository retains the modified Chat2DB license for adapted code. It is not the unmodified Apache 2.0 license. See [LICENSE](LICENSE) and [NOTICE](NOTICE); third-party dependencies retain their own licenses.
