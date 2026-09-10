mod results;
mod server;
use anyhow::Result;
use clap::Parser;
use fs2::FileExt;
use sqlx_core::{
    storage::{atomic_write, open_private, restrict, Store},
    ui::{UiState, UI_PROTOCOL},
};
use std::{fs, path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(name = "sqlx-ui", version)]
struct Options {
    #[arg(long)]
    data_dir: PathBuf,
    #[arg(long)]
    manifest: String,
    #[arg(long)]
    worker_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Local UI stopped: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let options = Options::parse();
    {
        Store::open(options.data_dir.clone())?;
    }
    let dir = options.data_dir.join("ui");
    fs::create_dir_all(&dir)?;
    restrict(&dir, true)?;
    let lock = open_private(&dir.join("server.lock"))?;
    lock.try_lock_exclusive()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let state = UiState {
        protocol: UI_PROTOCOL,
        instance: uuid::Uuid::new_v4().to_string(),
        origin: format!("http://{}", listener.local_addr()?),
        token: server::token(),
        pid: std::process::id(),
    };
    let app = Arc::new(server::App::new(
        state.clone(),
        options.data_dir,
        options.manifest,
        options.worker_dir,
    )?);
    atomic_write(&dir.join("state.json"), &serde_json::to_vec(&state)?)?;
    let maintenance = tokio::spawn(server::maintenance(app.clone()));
    axum::serve(listener, server::router(app.clone()))
        .with_graceful_shutdown(app.shutdown.clone().cancelled_owned())
        .await?;
    app.cancel_tasks();
    for _ in 0..100 {
        if app.active.load(std::sync::atomic::Ordering::Relaxed) == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    maintenance.abort();
    if let Ok(bytes) = fs::read(dir.join("state.json")) {
        if serde_json::from_slice::<UiState>(&bytes).is_ok_and(|s| s.instance == state.instance) {
            fs::remove_file(dir.join("state.json"))?;
        }
    }
    Ok(())
}
