mod components;
mod execution;
mod skill;
mod storage;

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
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Init,
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
}
#[derive(Subcommand)]
enum DatasourceCommand {
    Add {
        #[arg(long)]
        name: String,
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
    match run(cli) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            println!(
                "{}",
                json!({"success":false,"error":{"code":"sqlx.error","message":format!("{e:#}")}})
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
        Commands::Init => {
            let store = Store::open(root)?;
            print(json!({"initialized":true,"identity":store.identity}));
        }
        Commands::Datasource { command } => match command {
            DatasourceCommand::Add { name, connection } => {
                validate_name(&name)?;
                let connection = connection.resolve(None)?;
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
            } => {
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
                let c = connection.resolve(Some(sources[index].connection.clone()))?;
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
        },
        Commands::Sql {
            command:
                SqlCommand::Execute {
                    datasource,
                    statements,
                },
        } => {
            if statements.iter().any(|s| s.trim().is_empty()) {
                bail!("SQL statements must not be empty");
            }
            let source = {
                let store = Store::open(root.clone())?;
                store.find(&datasource)?
            };
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
    fn resolve(self, existing: Option<Connection>) -> Result<Connection> {
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
        } else if is_new {
            if !io::stdin().is_terminal() {
                bail!("use --username-env and --password-env, or --connection-stdin for noninteractive creation");
            }
            c.username = rpassword::prompt_password("Database username: ")?;
        }
        if let Some(name) = self.password_env {
            c.password =
                std::env::var(&name).context("password environment variable is unavailable")?;
        } else if is_new {
            if !io::stdin().is_terminal() {
                bail!("use --password-env or --connection-stdin for noninteractive creation");
            }
            c.password = rpassword::prompt_password("Database password: ")?;
        }
        Ok(c)
    }
}
