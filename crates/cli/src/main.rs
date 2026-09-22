mod skill;
use sqlx_core::{components, execution, mcp, plugins, prefetch, storage, ui, updates};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use sqlx_protocol::{Action, Connection, Database};
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};
use storage::{Datasource, Store};

#[derive(Parser)]
#[command(
    name = "sqlx",
    version = concat!(env!("CARGO_PKG_VERSION"), " (OtterMind/sqlx)"),
    about = "Database CLI for agents: encrypted datasources and complete SQL results"
)]
struct Cli {
    #[arg(long, global = true, env = "SQLX_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long,global=true,env="SQLX_MANIFEST",default_value=components::DEFAULT_MANIFEST)]
    manifest: String,
    /// Use locally built workers for development instead of release downloads.
    #[arg(long, global = true, env = "SQLX_WORKER_DIR", hide = true)]
    worker_dir: Option<PathBuf>,
    #[arg(long, global = true, env = "SQLX_UPDATE_RELEASE_BASE", hide = true, default_value = updates::RELEASE_BASE)]
    update_release_base: String,
    #[arg(long, global = true, env = "SQLX_UPDATE_DIR", hide = true)]
    update_dir: Option<PathBuf>,
    /// Return the local page URL without opening a browser.
    #[arg(long, global = true)]
    no_open: bool,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Init,
    /// Serve the Model Context Protocol over stdio so agent harnesses can call SQLX as a tool.
    Mcp,
    /// Download database workers, the JDBC runtime and the browser UI before they are needed.
    Prefetch {
        /// Components to download: mysql, mariadb, tidb, greatsql, oceanbase, starrocks, doris, postgres, cockroachdb, yugabytedb, opengauss, oracle, sqlserver, clickhouse, trino, tdengine, dameng, kingbase, redis, mongodb, sqlite, duckdb, h2, ui, skill or all.
        #[arg(
            value_name = "COMPONENT",
            required = true,
            num_args = 1..,
            value_parser = ["mysql", "mariadb", "tidb", "greatsql", "oceanbase", "starrocks", "doris", "postgres", "cockroachdb", "yugabytedb", "opengauss", "oracle", "sqlserver", "clickhouse", "trino", "tdengine", "dameng", "kingbase", "redis", "mongodb", "sqlite", "duckdb", "h2", "ui", "skill", "all"]
        )]
        components: Vec<String>,
    },
    /// Check or install official CLI updates.
    Update {
        #[command(subcommand)]
        command: UpdateCommand,
    },
    Datasource {
        #[command(subcommand)]
        command: DatasourceCommand,
    },
    Sql {
        #[command(subcommand)]
        command: SqlCommand,
    },
    /// Read the results this CLI stored, including the ones a command only previewed.
    Results {
        #[command(subcommand)]
        command: ResultsCommand,
    },
    /// Show or change settings such as the preview size and the result directory.
    Setting {
        #[command(subcommand)]
        command: SettingCommand,
    },
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Open or manage the local browser companion.
    Ui {
        #[command(subcommand)]
        command: Option<UiCommand>,
    },
}
#[derive(Subcommand)]
enum UpdateCommand {
    /// Fetch the latest stable version without installing it.
    Check,
    /// Download, verify and install the official CLI executable.
    Install {
        #[arg(long)]
        version: Option<String>,
    },
    /// Read the last check and installation result without network access.
    Status,
    #[command(hide = true)]
    BackgroundCheck,
}
#[derive(Subcommand)]
enum UiCommand {
    Status,
    Stop,
    /// Install and select community UI plugins.
    Plugin {
        #[command(subcommand)]
        command: UiPluginCommand,
    },
}
#[derive(Subcommand)]
enum UiPluginCommand {
    List,
    Install {
        #[arg(long, conflicts_with = "url", required_unless_present = "url")]
        path: Option<PathBuf>,
        #[arg(
            long,
            conflicts_with = "path",
            requires = "sha256",
            required_unless_present = "path"
        )]
        url: Option<String>,
        #[arg(long, requires = "url")]
        sha256: Option<String>,
    },
    Use {
        id: String,
        #[arg(long)]
        version: Option<String>,
    },
    Remove {
        id: String,
        #[arg(long)]
        version: String,
    },
}
#[derive(Subcommand)]
enum DatasourceCommand {
    Add {
        #[arg(long)]
        name: String,
        /// Let the user enter credentials in a local browser form.
        #[arg(long)]
        ui: bool,
        #[command(flatten)]
        connection: ConnectionArgs,
    },
    List,
    Show {
        #[arg(long)]
        id: String,
    },
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        ui: bool,
        #[command(flatten)]
        connection: ConnectionArgs,
    },
    Remove {
        #[arg(long)]
        id: String,
    },
    Test {
        #[arg(long)]
        id: String,
        /// Print the raw worker event stream instead of the table-shaped result.
        #[arg(long)]
        events: bool,
    },
    /// Check a browser setup request without retrieving credentials.
    SetupStatus {
        #[arg(long)]
        request_id: String,
    },
}
#[derive(Args, Default)]
struct ConnectionArgs {
    #[arg(long = "type")]
    database_type: Option<Database>,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    /// Database name, or the database file for sqlite, duckdb and embedded H2.
    #[arg(long, alias = "path")]
    database: Option<String>,
    #[arg(long)]
    service: Option<String>,
    #[arg(long,value_parser=["disable","verify-full"])]
    tls: Option<String>,
    /// Name of the environment variable containing the database username.
    #[arg(long)]
    username_env: Option<String>,
    /// Name of the environment variable containing the database password.
    #[arg(long)]
    password_env: Option<String>,
    /// Read a complete connection JSON object from stdin (including credentials).
    #[arg(long)]
    connection_stdin: bool,
    #[arg(long, value_name = "KEY=VALUE")]
    property: Vec<String>,
}
#[derive(Subcommand)]
enum SqlCommand {
    Execute {
        #[arg(long)]
        datasource: String,
        /// One complete statement or engine-native command; repeat for a batch.
        #[arg(
            long = "command",
            alias = "sql",
            required = true,
            allow_hyphen_values = true
        )]
        statements: Vec<String>,
        /// Execute once in the local UI service and open a paginated result page.
        #[arg(long)]
        view: bool,
        /// Rows printed per result set; more rows are stored and pointed at by `file`.
        #[arg(long, value_name = "ROWS")]
        preview: Option<u64>,
        /// Print every row and store nothing, for a caller that cannot read the stored file.
        #[arg(long)]
        full: bool,
        /// Print the raw worker event stream instead of the table-shaped result.
        #[arg(long)]
        events: bool,
    },
}
#[derive(Subcommand)]
enum ResultsCommand {
    /// List stored results, newest first.
    List {
        /// Show at most this many results.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Read rows of one stored result.
    Rows {
        #[arg(long)]
        id: String,
        /// Statement index inside the result, counted from zero.
        #[arg(long, default_value_t = 0)]
        statement: usize,
        /// Result set index of that statement, counted from zero.
        #[arg(long, default_value_t = 0)]
        set: usize,
        /// First row to return.
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// Rows to return, at most 200.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
}
#[derive(Subcommand)]
enum SettingCommand {
    /// Show every setting with its effective value and where it comes from.
    List,
    /// Show one setting.
    Get { key: String },
    /// Change one setting: preview-rows, results-dir or results-retention-hours.
    Set { key: String, value: String },
    /// Restore the default of one setting.
    Unset { key: String },
}
#[derive(Subcommand)]
enum SkillCommand {
    Install {
        #[arg(long, conflicts_with = "path", required_unless_present = "path")]
        target: Option<String>,
        #[arg(long, conflicts_with = "target", required_unless_present = "target")]
        path: Option<PathBuf>,
    },
    Update,
    Status,
    /// Stop managing a Skill installation; its files are left in place.
    Remove {
        #[arg(long)]
        path: PathBuf,
    },
}
fn main() {
    let cli = Cli::parse();
    let automatic = io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && !matches!(
            &cli.command,
            Commands::Update { .. } | Commands::Init | Commands::Prefetch { .. } | Commands::Mcp
        )
        && cli.worker_dir.is_none()
        && std::env::var("SQLX_NO_UPDATE_CHECK").ok().as_deref() != Some("1");
    let updater = if automatic {
        updates::Updater::new(cli.update_dir.clone(), cli.update_release_base.clone()).ok()
    } else {
        None
    };
    let outcome = run(cli);
    if let Some(updater) = updater {
        updater.notify_and_schedule();
    }
    match outcome {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            print_line(
                &json!({"success":false,"error":{"code":e.downcast_ref::<updates::UpdateError>().map(|e| e.code.as_str()).unwrap_or("sqlx.error"),"message":format!("{e:#}")}}),
            );
            std::process::exit(1);
        }
    }
}
fn run(cli: Cli) -> Result<bool> {
    let root = cli.data_dir.unwrap_or(
        dirs::home_dir()
            .context("cannot locate user home directory")?
            .join(".sqlx"),
    );
    match cli.command {
        Commands::Update { command } => {
            let updater = updates::Updater::new(cli.update_dir, cli.update_release_base)?;
            let value = match command {
                UpdateCommand::Check => updater.check(false)?,
                UpdateCommand::Install { version } => updater.install(version.as_deref())?,
                UpdateCommand::Status => updater.status()?,
                UpdateCommand::BackgroundCheck => {
                    updater.check(true)?;
                    return Ok(true);
                }
            };
            print(value);
        }
        Commands::Init => {
            let store = Store::open(root)?;
            print(json!({"initialized":true,"identity":store.identity}));
        }
        Commands::Mcp => {
            mcp::serve(root, cli.manifest, cli.worker_dir)?;
            return Ok(true);
        }
        Commands::Prefetch { components } => {
            let (report, complete) = prefetch::run(&root, &cli.manifest, &components)?;
            print(report);
            return Ok(complete);
        }
        Commands::Datasource { command } => match command {
            DatasourceCommand::Add {
                name,
                connection,
                ui: use_ui,
            } => {
                validate_name(&name)?;
                if use_ui {
                    let connection = connection.draft(None)?;
                    let client = ui::UiClient::start(root, cli.manifest, cli.worker_dir)?;
                    let mut result = client.post(
                        "/api/setups",
                        &ui::SetupRequest {
                            name,
                            source_id: None,
                            connection,
                        },
                    )?;
                    result["url"] = client
                        .page(
                            &format!(
                                "/setup/{}",
                                result["request_id"]
                                    .as_str()
                                    .context("missing request id")?
                            ),
                            !cli.no_open,
                        )?
                        .into();
                    print(result);
                    return Ok(true);
                }
                let connection = connection.resolve(None, true)?;
                connection.validate()?;
                let store = Store::open(root)?;
                let mut sources = store.load()?;
                if sources.iter().any(|s| s.name == name) {
                    bail!("datasource name already exists");
                }
                let datasource = Datasource {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    connection,
                };
                let public = datasource.public();
                sources.push(datasource);
                store.save(&sources)?;
                print(public);
            }
            DatasourceCommand::List => {
                let store = Store::open(root)?;
                print(
                    json!({"datasources":store.load()?.iter().map(Datasource::public).collect::<Vec<_>>()}),
                );
            }
            DatasourceCommand::Show { id } => {
                let store = Store::open(root)?;
                print(store.find(&id)?.public());
            }
            DatasourceCommand::Update {
                id,
                name,
                connection,
                ui: use_ui,
            } => {
                if use_ui {
                    let existing = {
                        let store = Store::open(root.clone())?;
                        store.find(&id)?
                    };
                    let name = name.unwrap_or(existing.name);
                    validate_name(&name)?;
                    let connection = connection.draft(Some(existing.connection))?;
                    let client = ui::UiClient::start(root, cli.manifest, cli.worker_dir)?;
                    let mut result = client.post(
                        "/api/setups",
                        &ui::SetupRequest {
                            name,
                            source_id: Some(existing.id),
                            connection,
                        },
                    )?;
                    result["url"] = client
                        .page(
                            &format!(
                                "/setup/{}",
                                result["request_id"]
                                    .as_str()
                                    .context("missing request id")?
                            ),
                            !cli.no_open,
                        )?
                        .into();
                    print(result);
                    return Ok(true);
                }
                let store = Store::open(root)?;
                let mut sources = store.load()?;
                let index = sources
                    .iter()
                    .position(|s| s.id == id || s.name == id)
                    .context("datasource not found")?;
                if let Some(name) = name {
                    validate_name(&name)?;
                    if sources
                        .iter()
                        .enumerate()
                        .any(|(i, s)| i != index && s.name == name)
                    {
                        bail!("datasource name already exists");
                    }
                    sources[index].name = name;
                }
                let c = connection.resolve(Some(sources[index].connection.clone()), true)?;
                c.validate()?;
                sources[index].connection = c;
                store.save(&sources)?;
                print(sources[index].public());
            }
            DatasourceCommand::Remove { id } => {
                let store = Store::open(root)?;
                let found = store.find(&id)?;
                let mut sources = store.load()?;
                sources.retain(|s| s.id != found.id);
                store.save(&sources)?;
                print(json!({"removed":found.id}));
            }
            DatasourceCommand::Test { id, events } => {
                let source = {
                    let store = Store::open(root.clone())?;
                    store.find(&id)?
                };
                let (out, effective) = execution::output_settings(&root, mode(events), None, None)?;
                let outcome = execution::run(
                    root,
                    cli.manifest,
                    cli.worker_dir,
                    source,
                    Action::Test,
                    vec![],
                    out,
                );
                execution::prune_results(&effective)?;
                return outcome;
            }
            DatasourceCommand::SetupStatus { request_id } => {
                uuid::Uuid::parse_str(&request_id).context("invalid setup request ID")?;
                let client = ui::UiClient::existing(&root)?
                    .context("local UI is not running; the setup request has expired")?;
                print(client.get(&format!("/api/setups/{request_id}/status"))?);
            }
        },
        Commands::Sql {
            command:
                SqlCommand::Execute {
                    datasource,
                    statements,
                    view,
                    preview,
                    full,
                    events,
                },
        } => {
            if statements.iter().any(|s| s.trim().is_empty()) {
                bail!("SQL statements must not be empty");
            }
            let source = {
                let store = Store::open(root.clone())?;
                store.find(&datasource)?
            };
            if view {
                let client = ui::UiClient::start(root, cli.manifest, cli.worker_dir)?;
                let mut result = client.post(
                    "/api/results",
                    &ui::ViewRequest {
                        request_id: uuid::Uuid::new_v4().to_string(),
                        datasource: source.id,
                        statements,
                    },
                )?;
                result["url"] = client
                    .page(
                        &format!(
                            "/result/{}",
                            result["result_id"].as_str().context("missing result id")?
                        ),
                        !cli.no_open,
                    )?
                    .into();
                print(result);
                return Ok(true);
            }
            let (out, effective) = execution::output_settings(
                &root,
                mode(events),
                preview,
                full.then_some(sqlx_core::settings::ResultMode::Full),
            )?;
            let outcome = execution::run(
                root,
                cli.manifest,
                cli.worker_dir,
                source,
                Action::Execute,
                statements,
                out,
            );
            execution::prune_results(&effective)?;
            return outcome;
        }
        Commands::Results { command } => {
            return results_command(&root, command);
        }
        Commands::Setting { command } => {
            Store::open(root.clone())?;
            return setting_command(&root, command);
        }
        Commands::Skill { command } => {
            {
                Store::open(root.clone())?;
            }
            let manager = components::Components::new(root, cli.manifest);
            let value = match command {
                SkillCommand::Install { target, path } => {
                    skill::install(&manager, skill::target_path(target, path)?)?
                }
                SkillCommand::Update => skill::update(&manager)?,
                SkillCommand::Status => skill::status(&manager)?,
                SkillCommand::Remove { path } => skill::remove(&manager, path)?,
            };
            print(value);
        }
        Commands::Ui { command } => match command {
            Some(UiCommand::Plugin { command }) => {
                {
                    Store::open(root.clone())?;
                }
                match command {
                    UiPluginCommand::List => print(json!({"plugins":plugins::list(&root)?})),
                    UiPluginCommand::Install { path, url, sha256 } => {
                        let manifest = if let Some(path) = path {
                            plugins::install(&root, &path)?
                        } else {
                            plugins::install_url(
                                &root,
                                &url.context("plugin URL is required")?,
                                &sha256.context("plugin SHA-256 is required")?,
                            )?
                        };
                        print(
                            json!({"installed":manifest,"next":"Select it with sqlx ui plugin use <id>"}),
                        );
                    }
                    UiPluginCommand::Use { id, version } => {
                        let selected = plugins::activate(&root, &id, version.as_deref())?;
                        print(json!({"active":selected,"reload_pages":true}));
                    }
                    UiPluginCommand::Remove { id, version } => {
                        let ui_dir = root.join("ui");
                        std::fs::create_dir_all(&ui_dir)?;
                        let lock = storage::open_private(&ui_dir.join("startup.lock"))?;
                        fs2::FileExt::lock_exclusive(&lock)?;
                        if ui::UiClient::existing(&root)?.is_some() {
                            bail!("stop the UI service before removing a plugin so open pages keep their assets");
                        }
                        plugins::remove(&root, &id, &version)?;
                        print(json!({"removed":id,"version":version}));
                    }
                }
            }
            None => {
                let client = ui::UiClient::start(root, cli.manifest, cli.worker_dir)?;
                print(json!({"url": client.page("/", !cli.no_open)?, "status": "running"}));
            }
            Some(UiCommand::Status) => {
                let state = ui::UiClient::existing(&root)?;
                print(match state {
                    Some(c) => {
                        json!({"status":"running","origin":c.state.origin,"pid":c.state.pid})
                    }
                    None => json!({"status":"stopped"}),
                });
            }
            Some(UiCommand::Stop) => {
                if let Some(client) = ui::UiClient::existing(&root)? {
                    print(client.post("/api/stop", &json!({}))?);
                } else {
                    print(json!({"status":"stopped"}));
                }
            }
        },
    }
    Ok(true)
}
/// Write one JSON line. A consumer that stops reading (`sqlx … | head`) must not turn
/// into a panic; any other write failure still reports itself and fails.
fn mode(events: bool) -> sqlx_core::output::Mode {
    if events {
        sqlx_core::output::Mode::Events
    } else {
        sqlx_core::output::Mode::Compact
    }
}
/// Stored results, all of them written by this CLI into the configured result directory.
fn results_command(root: &Path, command: ResultsCommand) -> Result<bool> {
    let settings = sqlx_core::settings::Settings::load(root)?;
    let effective = sqlx_core::settings::resolve(root, &settings, None, None)?;
    sqlx_core::results::prune(
        &effective.results_dir,
        sqlx_core::results::CLI_ORIGIN,
        effective.retention,
        sqlx_core::results::MAX_STORED_BYTES,
    )?;
    match command {
        ResultsCommand::List { limit } => {
            let stored = sqlx_core::results::stored(&effective.results_dir)?;
            let results: Vec<Value> = stored
                .iter()
                .take(limit)
                .map(|store| {
                    let rows: u64 = store.metadata.tables.iter().map(|table| table.rows).sum();
                    json!({
                        "id": store.metadata.result_id,
                        "datasource": store.metadata.datasource_name,
                        "status": store.metadata.status,
                        "created_at": store.metadata.created_at,
                        "rows": rows.to_string(),
                    })
                })
                .collect();
            print(json!({"dir": effective.results_dir.to_string_lossy(), "results": results}));
        }
        ResultsCommand::Rows {
            id,
            statement,
            set,
            offset,
            limit,
        } => {
            uuid::Uuid::parse_str(&id).context("id must be a result UUID")?;
            let store = sqlx_core::results::ResultStore::recover(&effective.results_dir.join(&id))
                .with_context(|| {
                    format!(
                        "result {id} is not stored in {}",
                        effective.results_dir.display()
                    )
                })?;
            let page = store.page(statement, set, offset, limit)?;
            let columns: Vec<Value> = store
                .metadata
                .tables
                .iter()
                .find(|table| table.statement == statement && table.result == set)
                .map(|table| {
                    table
                        .columns
                        .iter()
                        .map(sqlx_core::output::column_json)
                        .collect()
                })
                .unwrap_or_default();
            print(json!({
                "cols": columns,
                "rows": page.rows,
                "offset": page.offset.to_string(),
                "next_offset": page.next_offset.to_string(),
                "total": page.total_rows.to_string(),
                "complete": page.complete,
            }));
        }
    }
    Ok(true)
}
/// Show or change the settings stored next to the datasources.
fn setting_command(root: &Path, command: SettingCommand) -> Result<bool> {
    use sqlx_core::settings::{self, Key, Settings};
    let mut stored = Settings::load(root)?;
    let effective = settings::resolve(root, &stored, None, None)?;
    let describe = |effective: &settings::Effective, key: Key| -> Value {
        let (value, source) = match key {
            Key::PreviewRows => (effective.preview_rows.to_string(), effective.preview_source),
            Key::ResultsDir => (
                effective.results_dir.to_string_lossy().into_owned(),
                effective.results_dir_source,
            ),
            Key::RetentionHours => (
                effective
                    .retention
                    .map_or(0, |seconds| seconds / 3_600)
                    .to_string(),
                effective.retention_source,
            ),
            Key::ResultMode => (
                effective.result_mode.name().to_owned(),
                effective.result_mode_source,
            ),
        };
        json!({"key": key.name(), "value": value, "source": source.name()})
    };
    match command {
        SettingCommand::List => {
            let settings: Vec<Value> = Key::ALL
                .into_iter()
                .map(|key| describe(&effective, key))
                .collect();
            print(json!({
                "settings": settings,
                "path": settings::path(root).to_string_lossy(),
            }));
        }
        SettingCommand::Get { key } => {
            let key = Key::parse(&key)?;
            print(describe(&effective, key));
        }
        SettingCommand::Set { key, value } => {
            let key = Key::parse(&key)?;
            stored.set(key, &value)?;
            if key == Key::ResultsDir {
                // Fail now instead of during the first large result.
                let directory = PathBuf::from(stored.results_dir.clone().unwrap_or_default());
                settings::ensure_results_dir(&directory, false, root)?;
            }
            stored.save(root)?;
            let effective = settings::resolve(root, &stored, None, None)?;
            print(describe(&effective, key));
        }
        SettingCommand::Unset { key } => {
            let key = Key::parse(&key)?;
            stored.unset(key);
            stored.save(root)?;
            let effective = settings::resolve(root, &stored, None, None)?;
            print(describe(&effective, key));
        }
    }
    Ok(true)
}
fn print_line(value: &Value) {
    if let Err(error) = writeln!(io::stdout(), "{value}") {
        if error.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("sqlx could not write its result: {error}");
        std::process::exit(1);
    }
}
fn print(data: Value) {
    print_line(&json!({"success":true,"data":data}));
}
/// Resolve a local database file: expand `~`, then make it absolute so a saved datasource
/// keeps working from any directory.
fn local_database_path(value: &str) -> Result<String> {
    let expanded = if let Some(rest) = value.trim().strip_prefix("~/") {
        dirs::home_dir()
            .context("cannot resolve the home directory")?
            .join(rest)
    } else if value.trim() == "~" {
        dirs::home_dir().context("cannot resolve the home directory")?
    } else {
        PathBuf::from(value.trim())
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()?.join(expanded)
    };
    Ok(absolute.to_string_lossy().into_owned())
}
fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || uuid::Uuid::parse_str(name).is_ok() {
        bail!("datasource name must be nonempty and must not be a UUID");
    }
    Ok(())
}
impl ConnectionArgs {
    fn draft(self, existing: Option<Connection>) -> Result<Connection> {
        if self.password_env.is_some() || self.connection_stdin {
            bail!("--ui receives passwords in the browser; omit --password-env and --connection-stdin");
        }
        let mut connection = self.resolve(existing, false)?;
        connection.password.clear();
        Ok(connection)
    }
    fn resolve(self, existing: Option<Connection>, prompt: bool) -> Result<Connection> {
        if self.connection_stdin {
            if self.database_type.is_some()
                || self.host.is_some()
                || self.port.is_some()
                || self.database.is_some()
                || self.service.is_some()
                || self.tls.is_some()
                || self.username_env.is_some()
                || self.password_env.is_some()
                || !self.property.is_empty()
            {
                bail!("--connection-stdin cannot be combined with individual connection options");
            }
            return serde_json::from_reader(io::stdin().lock())
                .map_err(|_| anyhow!("stdin must contain a valid connection JSON object"));
        }
        let is_new = existing.is_none();
        let mut c = if let Some(c) = existing {
            c
        } else {
            let database_type = self.database_type.context("--type is required")?;
            Connection {
                database_type,
                // H2 opens a local file until a host is given; every other engine needs one.
                host: match database_type {
                    Database::H2 => String::new(),
                    _ => "localhost".into(),
                },
                port: match database_type {
                    Database::Mysql | Database::Mariadb => 3306,
                    Database::Tidb => 4000,
                    Database::Greatsql => 3306,
                    Database::Oceanbase => 2881,
                    Database::Postgresql | Database::Cockroachdb => 5432,
                    Database::Yugabytedb => 5433,
                    Database::Opengauss => 5432,
                    Database::Oracle => 1521,
                    Database::Sqlserver => 1433,
                    Database::Clickhouse => 8123,
                    Database::Trino => 8080,
                    Database::Starrocks | Database::Doris => 9030,
                    Database::Tdengine => 6041,
                    Database::Dameng => 5236,
                    Database::Kingbase => 54321,
                    Database::Redis => 6379,
                    Database::Mongodb => 27017,
                    // A local engine has no port, and H2 uses its file mode until a host is given.
                    Database::Sqlite | Database::Duckdb => 0,
                    Database::H2 => 9092,
                },
                database: String::new(),
                service: String::new(),
                username: String::new(),
                password: String::new(),
                tls: "verify-full".into(),
                properties: BTreeMap::new(),
            }
        };
        if let Some(t) = self.database_type {
            if t != c.database_type {
                bail!("changing database type requires creating a new datasource");
            }
        }
        if let Some(v) = self.host {
            c.host = v;
        }
        if let Some(v) = self.port {
            c.port = v;
        }
        if let Some(v) = self.database {
            c.database = v;
        }
        if let Some(v) = self.service {
            c.service = v;
        }
        if let Some(v) = self.tls {
            c.tls = v;
        }
        for entry in self.property {
            let (key, value) = entry
                .split_once('=')
                .context("--property requires KEY=VALUE")?;
            c.properties.insert(key.into(), value.into());
        }
        let local = c.is_local();
        if local {
            if c.database.is_empty() {
                bail!(
                    "{:?} needs --database <file> or --path <file>",
                    c.database_type
                );
            }
            c.database = local_database_path(&c.database)?;
            c.host = String::new();
            c.port = 0;
            c.username = String::new();
            c.password = String::new();
            c.service = String::new();
            c.tls = "disable".into();
        }
        if let Some(name) = self.username_env {
            c.username =
                std::env::var(&name).context("username environment variable is unavailable")?;
        } else if is_new && prompt && !local {
            if !io::stdin().is_terminal() {
                bail!("use --username-env and --password-env, or --connection-stdin for noninteractive creation");
            }
            c.username = rpassword::prompt_password("Database username: ")?;
        }
        if let Some(name) = self.password_env {
            c.password =
                std::env::var(&name).context("password environment variable is unavailable")?;
        } else if is_new && prompt && !local {
            if !io::stdin().is_terminal() {
                bail!("use --password-env or --connection-stdin for noninteractive creation");
            }
            c.password = rpassword::prompt_password("Database password: ")?;
        }
        if local {
            c.username = String::new();
            c.password = String::new();
        }
        Ok(c)
    }
}
