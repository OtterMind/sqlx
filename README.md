# SQLX

Connect to MySQL, PostgreSQL, Oracle, and SQL Server from your terminal or agent. Save encrypted connections, run one or more SQL statements, and receive complete structured results.

Start with the CLI, or install the [Skill](skills/sqlx/SKILL.md) first and let your agent set up the CLI.

Download prebuilt packages from [GitHub Releases](https://github.com/OtterMind/sqlx/releases), or use the installers below. See [build from source](#build-from-source) for development.

The [v0.1.0 release acceptance report](docs/release-acceptance-v0.1.0.md) records public-download installation, both CLI/Skill entry paths, and tests against four local databases, including the scope and limits of validation.

## Install the CLI

### macOS and Linux

Run:

```sh
curl -fsSL https://raw.githubusercontent.com/OtterMind/sqlx/main/scripts/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
sqlx --version
sqlx init
```

The installer selects your platform, downloads the executable from GitHub Releases, and verifies its SHA-256. It installs to `~/.local/bin`. Add that directory to your shell's persistent PATH for future sessions. Set `SQLX_INSTALL_DIR` to choose another directory or `SQLX_VERSION` to select a release version.

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

Add `%LOCALAPPDATA%\Programs\SQLX` to your user PATH for future sessions. The installer verifies the download before installing. Both installers preserve an unrelated executable already named `sqlx`; use a different install directory in that case.

Prebuilt targets are macOS ARM64/x64, Linux ARM64/x64, and Windows x64. The initial Linux build baseline is Ubuntu 24.04; older distributions have not been verified. Release users do not need to install Rust, Node.js, Java, or database drivers separately. Database workers and a private JRE are downloaded only when needed.

## Install the Skill

### If the CLI is already installed

Choose the agent you use:

```sh
# Codex
sqlx skill install --target codex

# Claude Code
sqlx skill install --target claude
```

For another agent, provide its complete skill directory:

```sh
sqlx skill install --path /path/to/agent/skills/sqlx
sqlx skill status
```

The CLI downloads the Skill from GitHub Releases. Reload skills or start a new agent session according to your agent's discovery mechanism. Later, `sqlx skill update` updates managed installations while preserving local edits.

### If you want to install the Skill first

The Skill source is available now and does not require the CLI. On macOS or Linux, install it for Codex with:

```sh
sqlx_checkout="$(mktemp -d)"
git clone --depth 1 https://github.com/OtterMind/sqlx.git "$sqlx_checkout"
mkdir -p "$HOME/.agents/skills"
if [ ! -e "$HOME/.agents/skills/sqlx" ]; then
  cp -R "$sqlx_checkout/skills/sqlx" "$HOME/.agents/skills/sqlx"
else
  echo "Skill directory already exists; existing files were preserved."
fi
```

For Claude Code, use `~/.claude/skills/sqlx` as the destination. On Windows, the equivalent PowerShell steps for Codex are:

```powershell
$checkout = Join-Path $env:TEMP ('sqlx-skill-' + [guid]::NewGuid())
git clone --depth 1 https://github.com/OtterMind/sqlx.git $checkout
$skillRoot = Join-Path $env:USERPROFILE '.agents\skills'
New-Item -ItemType Directory -Force $skillRoot | Out-Null
if (-not (Test-Path (Join-Path $skillRoot 'sqlx'))) {
  Copy-Item -Recurse (Join-Path $checkout 'skills\sqlx') $skillRoot
} else {
  Write-Output 'Skill directory already exists; existing files were preserved.'
}
```

For another agent, copy the entire `skills/sqlx` folder, including `references`, into the agent's supported skill directory. Manual source installations remain manually managed; the CLI will not overwrite them as if it owned them.

After your agent discovers the Skill, you can ask:

> Use the SQLX skill to install the OtterMind SQLX CLI if needed, then help me add and test a PostgreSQL connection. Ask me for missing connection details.

The Skill includes [CLI installation instructions](skills/sqlx/references/install-cli.md), so the agent can check its environment and follow the appropriate installation path.

## Local browser pages (0.2.0 development)

These commands are being developed for 0.2.0 and are not included in the published 0.1.0 release. Build this branch from source to try them before 0.2.0 is released.

Let the user enter the password directly in a local page. The CLI pre-fills the known connection settings:

```sh
sqlx datasource add --ui --name dev --type postgresql --host db.example.com --port 5432 --database app
```

The command returns immediately with a page URL and `request_id`. The user fills in their credentials and chooses **Save & connect**. SQLX verifies the connection and encrypts the saved configuration. The agent checks completion without receiving the password:

```sh
sqlx datasource setup-status --request-id <request-id>
```

To edit an existing connection while keeping its saved password unless explicitly replaced:

```sh
sqlx datasource update --id dev --ui
```

To send an already written query to a results page:

```sh
sqlx sql execute --datasource dev --sql "SELECT id, name FROM users ORDER BY id" --view
```

SQLX executes once and returns a result URL. The page loads the results automatically, supports multiple result sets and pagination, and preserves exact values. Refreshing, paging, or reopening the page reads the same cached result rather than executing the SQL again. Results are retained locally for 24 hours, with owner-restricted permissions. The usual CLI execution mode still streams complete JSON to stdout.

`sqlx ui` opens the local workspace; `sqlx ui status` and `sqlx ui stop` inspect or stop it. `--no-open` returns a link without launching a browser. Pages are accessible on the same machine as SQLX. The local UI service and default UI plugin are separate packages, downloaded only when needed. Setup links expire after five minutes unless already exchanged for a browser session; unfinished setup requests expire after 30 minutes. The service exits after 30 idle minutes when no task is active.

Password entry through the page keeps credentials out of the normal agent conversation and tool response. It does not isolate credentials from an agent that can read files or control the browser as the same operating-system user. See [the local UI design](docs/local-ui.md) for the interface and storage boundaries.

### Choose your UI

The default interface is a plugin. You can install a community interface and switch without changing saved connections or rerunning queries:

```sh
sqlx ui plugin install --url <plugin-zip-url> --sha256 <published-sha256>
sqlx ui plugin list
sqlx ui plugin use <plugin-id>
```

Installation does not activate a plugin. After selecting it, reload an open page or run `sqlx ui`. To return to the default interface, run `sqlx ui plugin use default`. To remove an inactive version, stop the service with `sqlx ui stop`, then run `sqlx ui plugin remove <plugin-id> --version <version>`.

Plugins run locally and can access entered credentials and displayed data. Install interfaces from authors you trust; a checksum verifies the downloaded bytes, not the author's trustworthiness. SQLX validates API/CLI compatibility and preserves installed versions.

To build your own interface, see the [UI plugin guide](docs/ui-plugins.md), [typed browser SDK](ui/sdk/client.ts), and independent [terminal UI example](examples/terminal-ui/). Any framework that produces static browser assets can use the API. No Node.js runtime is needed by users.

## First connection and query

Create a PostgreSQL connection. Replace the host and database with your own values; an interactive terminal prompts for the username and password:

```sh
sqlx datasource add --name dev --type postgresql --host db.example.com --port 5432 --database app
sqlx datasource test --id dev
sqlx sql execute --datasource dev --sql "SELECT current_database()" --sql "SELECT 1"
```

For an agent or script, provide credentials through environment variables or a connection JSON object on stdin, as described below. TLS verifies the database certificate by default; use `--tls disable` only for a connection explicitly intended to be unencrypted.

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

## Local data and downloaded resources

User data lives in `~/.sqlx/`. Use `--data-dir` or `SQLX_DATA_DIR` for another location. Saved connections use AES-256-GCM with an independently generated local key. Back up the key together with the encrypted data; losing the key prevents decryption. Device identity is generated locally, and this version does not upload device information.

The main executable contains no database drivers. MySQL and PostgreSQL use separate native Rust workers; Oracle and SQL Server use a separate JDBC worker. Downloaded resources are selected from a compatible GitHub Release manifest and verified before use. `--manifest <https-url>` selects a particular manifest or local test server.

The [database references](skills/sqlx/references/) explain each SQL operation's purpose, parameters, result, and official documentation link.

## Build from source

Source development requires Git, Rust 1.95, Node.js 22, and the platform's native build tools. Node.js only builds UI plugin assets; release users do not need it. For Oracle or SQL Server development, also install a Java 17 JDK and Maven.

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

Keep the checkout at that location, or copy the CLI and both native workers into a dedicated directory and update PATH and `SQLX_WORKER_DIR` accordingly. Add these settings to future sessions when needed. This source-build setup makes MySQL and PostgreSQL usable without a published worker manifest.

For Oracle and SQL Server, build the JDBC worker and place its driver JARs alongside those workers. On macOS or Linux:

```sh
mvn -B -f java/jdbc/pom.xml package
cp java/jdbc/target/sqlx-jdbc-0.2.0.jar target/release/sqlx-jdbc.jar
curl -fL https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar -o target/release/ojdbc.jar
curl -fL https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar -o target/release/mssql-jdbc.jar
```

On Windows PowerShell:

```powershell
mvn -B -f java/jdbc/pom.xml package
Copy-Item java/jdbc/target/sqlx-jdbc-0.2.0.jar target/release/sqlx-jdbc.jar
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
docker compose -f tests/compose.yaml up -d --wait
python3 tests/integration.py
python3 tests/ui_api.py
docker compose -f tests/compose.yaml down -v
```

For local native workers, set `SQLX_WORKER_DIR` to the absolute `target/debug` directory. For JDBC development, that directory also contains `sqlx-jdbc.jar` and `ojdbc.jar` or `mssql-jdbc.jar`; `SQLX_JAVA_BIN` can select Java 17 explicitly. These overrides are for development, not prerequisites for release users. The fixture scripts use dedicated test containers and test-only credentials.

See [the design](docs/design.md) for implementation boundaries and deferred features. SQL-file input, persistent sessions, configurable transaction/error modes, result-file export, automatic CLI updates, and telemetry are not v1 features.

## License

This public source repository retains the modified Chat2DB license for adapted code. It is not the unmodified Apache 2.0 license. See [LICENSE](LICENSE) and [NOTICE](NOTICE); third-party dependencies retain their own licenses.
