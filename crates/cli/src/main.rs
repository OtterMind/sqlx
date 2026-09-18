mod skill;
use sqlx_core::{components, execution, plugins, prefetch, storage, ui, updates};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use sqlx_protocol::{Action, Connection, Database};
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal},
    path::PathBuf,
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
    /// Download database workers, the JDBC runtime and the browser UI before they are needed.
    Prefetch {
        /// Components to download: mysql, postgres, oracle, sqlserver, ui, skill or all.
        #[arg(
            value_name = "COMPONENT",
            required = true,
            num_args = 1..,
            value_parser = ["mysql", "postgres", "oracle", "sqlserver", "ui", "skill", "all"]
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
    #[arg(long)]
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
        #[arg(long = "sql", required = true, allow_hyphen_values = true)]
        statements: Vec<String>,
        /// Execute once in the local UI service and open a paginated result page.
        #[arg(long)]
        view: bool,
    },
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
}
fn main() {
    let cli = Cli::parse();
    let automatic = io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && !matches!(
            &cli.command,
            Commands::Update { .. } | Commands::Init | Commands::Prefetch { .. }
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
            println!(
                "{}",
                json!({"success":false,"error":{"code":e.downcast_ref::<updates::UpdateError>().map(|e| e.code.as_str()).unwrap_or("sqlx.error"),"message":format!("{e:#}")}})
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
            DatasourceCommand::Test { id } => {
                let source = {
                    let store = Store::open(root.clone())?;
                    store.find(&id)?
                };
                return execution::run(
                    root,
                    cli.manifest,
                    cli.worker_dir,
                    source,
                    Action::Test,
                    vec![],
                );
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
            return execution::run(
                root,
                cli.manifest,
                cli.worker_dir,
                source,
                Action::Execute,
                statements,
            );
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
fn print(data: Value) {
    println!("{}", json!({"success":true,"data":data}));
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
                host: "localhost".into(),
                port: match database_type {
                    Database::Mysql => 3306,
                    Database::Postgresql => 5432,
                    Database::Oracle => 1521,
                    Database::Sqlserver => 1433,
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
        if let Some(name) = self.username_env {
            c.username =
                std::env::var(&name).context("username environment variable is unavailable")?;
        } else if is_new && prompt {
            if !io::stdin().is_terminal() {
                bail!("use --username-env and --password-env, or --connection-stdin for noninteractive creation");
            }
            c.username = rpassword::prompt_password("Database username: ")?;
        }
        if let Some(name) = self.password_env {
            c.password =
                std::env::var(&name).context("password environment variable is unavailable")?;
        } else if is_new && prompt {
            if !io::stdin().is_terminal() {
                bail!("use --password-env or --connection-stdin for noninteractive creation");
            }
            c.password = rpassword::prompt_password("Database password: ")?;
        }
        Ok(c)
    }
}
