# SQLX

Connect to MySQL, MariaDB, TiDB, GreatSQL, OceanBase, PostgreSQL, CockroachDB, YugabyteDB, openGauss, Oracle, SQL Server, ClickHouse, Trino, Presto, StarRocks, Apache Doris, TDengine, Dameng, KingbaseES, Apache Kylin, XuguDB, IBM Db2, IBM Informix, SUNDB, GBase 8s, Redis and MongoDB, or open a local SQLite, DuckDB or H2 file, from your terminal or from your agent. Connections are saved encrypted, one invocation runs one or more statements or commands, and the results come back complete and structured.

## Quick start

```sh
# 1. Install the CLI (macOS, Linux and Windows x64; Node.js 22 or newer)
npx -y @ottermind/sqlx@latest
export PATH="$HOME/.local/bin:$PATH"

# 2. Create a connection (an interactive terminal prompts for the username and password)
sqlx datasource add --name dev --type postgresql --host db.example.com --port 5432 --database app
sqlx datasource test --id dev

# 3. Run SQL
sqlx sql execute --datasource dev --command "SELECT current_database()"
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

`sqlx update install --version <version>` installs an exact stable version. Updates do not stop running SQL or the UI service, do not modify saved connections, and do not update Skills or plugins automatically; use `sqlx skill update` for managed Skills, and an installed UI plugin keeps the version you selected (the default interface moves with the CLI release).

Interactive use checks in the background at most once per day and only prints a notice on stderr. Piped and CI commands skip that check, and `SQLX_NO_UPDATE_CHECK=1` disables it. Source builds and package-manager-owned paths keep their own installation method.

## Use with your agent

All four are ready as soon as they are installed: the plugin or extension installs the `sqlx` CLI
itself when it is missing, so you never install it separately. Codex and Claude additionally
require the CLI to meet the plugin's minimum version and refuse to start an older incompatible MCP
server silently.

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
sqlx sql execute --datasource dev --command "SELECT current_database()" --command "SELECT 1"
```

| Database | `--type` values | Execution | Required values and defaults |
|---|---|---|---|
| MySQL | `mysql` | native worker | port 3306 by default |
| MariaDB | `mariadb` | reuses the MySQL worker | port 3306 by default |
| TiDB | `tidb` | reuses the MySQL worker | port 4000 by default |
| GreatSQL | `greatsql` | reuses the MySQL worker | port 3306 by default |
| OceanBase | `oceanbase`, `ob` | reuses the MySQL worker | port 2881 by default; connect as `user@tenant` |
| StarRocks | `starrocks` | reuses the MySQL worker | port 9030 by default; has no user database until one is created |
| Apache Doris | `doris` | reuses the MySQL worker | port 9030 by default; has no user database until one is created |
| PostgreSQL | `postgresql`, `postgres`, `pgsql` | native worker | port 5432 by default |
| CockroachDB | `cockroachdb`, `cockroach`, `crdb` | reuses the PostgreSQL worker | |
| YugabyteDB | `yugabytedb`, `yugabyte`, `yb` | reuses the PostgreSQL worker | port 5433 by default |
| openGauss | `opengauss`, `gaussdb` | JDBC worker | port 5432 by default; authenticates with its own driver |
| Oracle | `oracle` | JDBC worker | `--service <service-name>` is required |
| SQL Server | `sqlserver`, `mssql` | JDBC worker | port 1433 by default |
| ClickHouse | `clickhouse` | JDBC worker | connects to the HTTP port, 8123 by default |
| Trino | `trino` | JDBC worker | `--database <catalog>[.<schema>]` is required |
| TDengine | `tdengine`, `taos` | JDBC worker | connects through taosAdapter, 6041 by default |
| Dameng | `dameng`, `dm` | JDBC worker | port 5236 by default; the account is also the default schema |
| KingbaseES | `kingbase`, `kingbasees` | JDBC worker | port 54321 by default |
| Redis | `redis` | native worker | port 6379 by default; each `--command` is one Redis command, not SQL |
| MongoDB | `mongodb`, `mongo` | native worker | port 27017 by default; each `--command` is one command document, not SQL |
| SQLite | `sqlite`, `sqlite3` | native worker | `--path <file>` (or `--database`) opens or creates a local file; no host, port or credentials; `--property mode=ro` and `--property busy_timeout=<ms>` |
| DuckDB | `duckdb` | native worker | `--path <file>` opens or creates a local file, `:memory:` keeps one for the invocation; `--property read_only=true` and `--property threads=<n>` |
| H2 | `h2` | JDBC worker | `--path <file>` opens a local file, or `--host` and `--port` (9092 by default) reach a TCP server |
| Presto | `presto`, `prestodb` | JDBC worker | port 8080 by default; `--database <catalog>[.<schema>]` is required; `--username` is required and a password is only sent over TLS |
| Hive | `hive` | JDBC worker | port 10000 by default; `--database` selects the Hive database, `default` when unset |
| Apache Kylin | `kylin` | JDBC worker | port 7070 by default; `--database` carries the Kylin project, and the default account is `ADMIN`/`KYLIN` |
| XuguDB | `xugu`, `xugudb` | JDBC worker | port 5138 by default; `SYSTEM` is the system database |
| IBM Db2 | `db2`, `ibmdb2` | JDBC worker | port 50000 by default; needs a driver you provide, see [Drivers you provide](#drivers-you-provide) |
| IBM Informix | `informix`, `ifx` | JDBC worker | port 9088 by default; `--service <server-instance>` is required and the driver must be provided |
| SUNDB | `sundb` | JDBC worker | port 22581 by default; runs the Goldilocks engine and needs the driver from the vendor image |
| GBase 8s | `gbase8s`, `gbasedbt` | JDBC worker | port 9088 by default; `--service <server-instance>` is required and the driver must be provided |

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

### Importing connections

A tool that already stores connections can hand all of them over in one versioned document, so a migration never needs a password in a command argument. [examples/import-connections.json](examples/import-connections.json) is a runnable starting point:

```sh
sqlx datasource import --stdin < connections.json
```

```json
{
  "version": 1,
  "mode": "merge",
  "datasources": [
    {
      "name": "dev",
      "connection": {
        "database_type": "postgresql",
        "host": "localhost",
        "port": 5432,
        "database": "app",
        "username": "example_account",
        "password": "replace_with_real_input",
        "tls": "verify-full"
      }
    }
  ]
}
```

Each `connection` is the same object `--connection-stdin` accepts. `merge` (the default) adds new names and updates existing ones in place while keeping their IDs, and it never deletes a stored connection. Every entry is validated on its own, so an unsupported engine or an incomplete connection is reported in `skipped` with its reason instead of failing the document:

```json
{"success":true,"data":{"mode":"merge","added":1,"updated":0,"unchanged":0,"total":1,"datasources":[{"id":"...","name":"dev","connection":{"database_type":"postgresql"}}],"skipped":[{"name":"legacy","reason":"invalid_connection","detail":"..."}]}}
```

`--dry-run` validates and reports without storing anything, `--strict` refuses the whole document when any entry is invalid, and `--file <path>` reads the document from a file for callers that cannot pipe stdin. The store is written once, so an import is never half applied.

### Commands

| Operation | Command |
|---|---|
| Initialize local storage | `sqlx init` |
| Create a connection | `sqlx datasource add --name dev --type mysql --host localhost --database app --username-env DB_USER --password-env DB_PASSWORD` |
| List connections | `sqlx datasource list` |
| Inspect a connection | `sqlx datasource show --id dev` |
| Change connection settings | `sqlx datasource update --id dev --host db.example.com` |
| Import saved connections | `sqlx datasource import --stdin` |
| Remove a saved connection | `sqlx datasource remove --id dev` |
| Test connectivity | `sqlx datasource test --id dev` |
| Execute SQL | `sqlx sql execute --datasource dev --command "SELECT 1" --command "SELECT 2"` |
| Download workers, the JDBC runtime and the UI ahead of time | `sqlx prefetch mysql ui` (`mariadb`, `tidb`, `greatsql`, `oceanbase`, `starrocks`, `doris`, `postgres`, `cockroachdb`, `yugabytedb`, `opengauss`, `oracle`, `sqlserver`, `clickhouse`, `trino`, `tdengine`, `dameng`, `kingbase`, `redis`, `mongodb`, `sqlite`, `duckdb`, `h2`, `skill` or `all`) |
| Execute and open a result page | `sqlx sql execute --datasource dev --command "SELECT 1" --view` |
| Read a stored result | `sqlx results list`, `sqlx results rows --id <result-id> --offset 100 --limit 50` |
| Show or change settings | `sqlx setting list`, `sqlx setting set preview-rows 20`, `sqlx setting set results-dir ~/sqlx-results` |
| Return a large result inline | `sqlx sql execute --datasource dev --command "SELECT …" --full` |
| Local workbench | `sqlx ui`, `sqlx ui status`, `sqlx ui stop` |
| Serve MCP over stdio | `sqlx mcp` |
| Install the Skill | `sqlx skill install --target codex`, `--target claude`, `--target dsh` or `--target pi` |
| Install to another skill directory | `sqlx skill install --path /path/to/skills/sqlx` |
| Inspect and update managed Skills | `sqlx skill status`, `sqlx skill update` |
| Stop managing a Skill installation | `sqlx skill remove --path /path/to/skills/sqlx` (files are kept) |
| Help and version | `sqlx --help`, `sqlx --version` |

### Execution behavior

Each invocation owns one database connection. Repeated `--command` arguments execute in order, initially with autocommit, and stop at the first error; there is no implicit all-or-nothing transaction. Temporary tables and session variables do not survive another invocation. Do not submit client directives such as `GO`, `DELIMITER` or psql backslash commands.

`--command` is the current flag and `--sql` is still accepted as an alias.

Output is one JSON object with one item per executed statement: `results[].stmt` identifies the statement, `cols` lists the columns as `[name, type]` pairs (`base64`, `boolean` and `json` join as a third entry when values are not plain text), `rows` holds positional values and `count` holds the driver's row count. Duplicate labels remain distinct. Numbers are encoded as strings to preserve integer and decimal precision, and binary data uses Base64. A failed statement carries its own `error` with an `outcome`; statements that never ran are listed in `skipped`; the object ends with `success`, which is also the process exit status.

Only a preview travels through standard output. A result set with more rows than `preview-rows` (10 by default) or more than 16 KiB of values is written completely to `results/<id>/<statement>-<result>.jsonl` inside the result directory, and its item carries a `file` path plus the object's `id`. Read more rows from that file, or page them with `sqlx results rows --id <id> --offset 100`. The default result directory is a private directory inside the system temporary directory, so results disappear when the machine reboots; `sqlx setting set results-dir ~/sqlx-results` keeps them, and `sqlx setting set results-retention-hours 0` stops the 24-hour cleanup.

A caller that cannot read files needs the whole result in one answer: `--full` prints every row and stores nothing, `sqlx setting set result-mode full` makes that the default for this machine, and the MCP tool `sqlx_sql_execute` takes `"full": true` for one call. A large result then fills the caller's own output budget, which is why the preview is the default.

`--events` prints the raw worker event stream instead (`protocol_version`, `datasource_id`, `events`, `success`) for scripts that parse it, and never stores anything. `--preview <rows>` overrides the preview size for one call.

Check the final `success` flag and the exit status, and never replay an uncertain write automatically.

## Local pages

Let the user inspect results in a browser and enter the password there.

```sh
sqlx sql execute --datasource dev --command "SELECT id, name FROM users ORDER BY id" --view
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

Installing does not activate a plugin; after selecting it, reload an open page or run `sqlx ui`. CLI updates keep the version of a plugin you installed and move the default interface to the version of the release, and switching versions needs `sqlx ui stop` first. Plugins run locally and can access entered credentials and displayed data, so install interfaces from authors you trust: a checksum proves the downloaded bytes are intact, not that the author is trustworthy.

To build your own interface, see the [UI plugin guide](docs/ui-plugins.md), the [typed browser SDK](ui/sdk/client.ts) and the independent [terminal UI example](examples/terminal-ui/). Users need no Node.js runtime.

## Data and downloads

User data lives in `~/.sqlx/`; use `--data-dir` or `SQLX_DATA_DIR` for another location. Settings are stored in `~/.sqlx/settings.json` and managed with `sqlx setting list|get|set|unset`; `SQLX_PREVIEW_ROWS`, `SQLX_RESULTS_DIR`, `SQLX_RESULTS_RETENTION_HOURS` and `SQLX_RESULT_MODE` override the file for one environment, and a command line flag overrides both. Stored results live in the result directory described above, keep 24 hours by default and stay under 1 GiB in total; older results are removed before the next command runs, and page results from `--view` are managed by the local service. Saved connections use AES-256-GCM with an independently generated local key: back up the key together with the encrypted data, because losing the key prevents decryption. Device identity is generated locally and this version uploads no device information.

The main executable contains no database drivers; each database's worker is downloaded on first use. MySQL, MariaDB, TiDB, GreatSQL, OceanBase, StarRocks and Apache Doris share the MySQL worker, Redis, MongoDB, SQLite and DuckDB run in their own native workers, PostgreSQL, CockroachDB and YugabyteDB share the PostgreSQL worker, and Oracle, SQL Server, ClickHouse, Trino, Presto, TDengine, openGauss, Dameng, KingbaseES, H2, Hive, Apache Kylin, XuguDB, IBM Db2, IBM Informix, SUNDB and GBase 8s use the JDBC worker (the [database table](#create-a-connection) lists which worker serves which database). Downloaded resources come from the fixed release manifest of the running CLI version and are verified before use; `--manifest <https-url>` selects another manifest or a local test server.

Downloads happen on first use and are cached afterwards. Each one prints `Downloading …` with speed and estimated time, and a final `Downloaded … in 12.3s (390 KB/s)` line on stderr; the progress line is refreshed only when stderr is a terminal, so piped JSON stays clean. An interrupted transfer is retried up to three times, and rerunning a failed command reuses every component that is already installed. To avoid waiting inside the first query or page:

```sh
sqlx prefetch mysql ui      # MySQL worker and the local browser UI
sqlx prefetch all           # adds the PostgreSQL, CockroachDB, YugabyteDB, openGauss, MariaDB, TiDB, GreatSQL, OceanBase, StarRocks, Doris, Oracle, SQL Server, ClickHouse, Trino, Presto, Hive, Kylin, XuguDB, TDengine, Dameng, KingbaseES, Redis, MongoDB, SQLite, DuckDB and H2 components, the JDBC runtime and the JRE
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
cp java/jdbc/target/sqlx-jdbc-0.1.16.jar target/release/sqlx-jdbc.jar
curl -fL https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar -o target/release/ojdbc.jar
curl -fL https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar -o target/release/mssql-jdbc.jar
```

On Windows PowerShell:

```powershell
mvn -B -f java/jdbc/pom.xml package
Copy-Item java/jdbc/target/sqlx-jdbc-0.1.16.jar target/release/sqlx-jdbc.jar
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
docker compose -f tests/compose.yaml up -d --wait greatsql tdengine
python3 tests/databases.py greatsql tdengine
docker compose -f tests/compose.yaml down -v
# Dameng and KingbaseES have no public image; point the fixtures at a local instance
# and export SQLX_TEST_DAMENG_PASSWORD or SQLX_TEST_KINGBASE_PASSWORD when they differ
python3 tests/databases.py dameng kingbase
docker compose -f tests/compose.yaml up -d --wait presto hive
bash scripts/jdbc-fixture.sh presto
bash scripts/jdbc-fixture.sh hive
python3 tests/databases.py presto hive
docker compose -f tests/compose.yaml down -v
# Kylin, XuguDB, Db2 and Informix take turns because each needs several gigabytes
docker compose -f tests/compose.yaml up -d --wait kylin
bash scripts/jdbc-fixture.sh kylin
python3 tests/databases.py kylin
docker compose -f tests/compose.yaml down -v
# SUNDB needs a licensed installation, and GBase 8s a vendor driver:
# export SQLX_TEST_SUNDB_PORT or SQLX_TEST_SUNDB_PASSWORD for the former, and
# SQLX_TEST_GBASE8S_DRIVER, SQLX_TEST_GBASE8S_PORT or SQLX_TEST_GBASE8S_PASSWORD for the latter
python3 tests/databases.py sundb gbase8s
```

For local native workers, set `SQLX_WORKER_DIR` to the absolute `target/debug` directory. For JDBC development, that directory also contains `sqlx-jdbc.jar` and `ojdbc.jar` or `mssql-jdbc.jar`; `SQLX_JAVA_BIN` can select Java 17 explicitly. These overrides are for development, not prerequisites for release users. The fixture scripts use dedicated test containers and test-only credentials.

## Drivers you provide

Most JDBC drivers ship with the release and are downloaded on first use. Vendors that do not allow
their driver to be redistributed are not packaged: IBM Db2, IBM Informix, SUNDB and GBase 8s connect
with a driver you install once from the vendor.

```sh
sqlx driver add --type db2 --jar ~/Downloads/jcc-12.1.0.0.jar
sqlx driver list
sqlx driver remove --type db2
```

`sqlx driver add` copies the jar into `<data-dir>/drivers/<engine>/`, checks that it really carries the
engine's driver class, and every later command loads it from there. A jar you provide also wins over a
released component, which is how a newer vendor driver is used before the release catches up. Running a
statement for one of these engines without a driver explains the exact command to run:

```sh
sqlx sql execute --datasource <id> --command "SELECT 1 FROM SYSIBM.SYSDUMMY1"
# SQLX does not redistribute the db2 driver; run `sqlx driver add --type db2 --jar <path>` with the vendor driver jar first
```

Where the drivers come from:

| Engine | File to provide | Driver class |
| --- | --- | --- |
| IBM Db2 | `jcc-<version>.jar` from IBM or Maven Central (`com.ibm.db2:jcc`) | `com.ibm.db2.jcc.DB2Driver` |
| IBM Informix | the Informix JDBC driver, 4.50 line (`com.ibm.informix:jdbc`); the 15.x line fails against an Informix 14.10 server | `com.informix.jdbc.IfxDriver` |
| SUNDB | `goldilocks8.jar` from the vendor image at `/goldilocks_home/lib/` | `sunje.goldilocks.jdbc.GoldilocksDriver` |
| GBase 8s | the vendor's `ifxjdbc.jar`; a wrapper jar that contains it must be unpacked first | `com.gbasedbt.jdbc.IfxDriver` |

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
