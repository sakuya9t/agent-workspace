mod api;
mod backend;
mod config;
mod db;
mod domain;
mod plugins;
mod session_manager;
mod source_control;
mod util;
mod workspace;

use std::sync::Arc;

use anyhow::{Context, Result};

use api::AppState;
use backend::native::NativePtyBackend;
use config::Config;
use db::Db;
use plugins::PluginRegistry;
use session_manager::SessionManager;
use util::now_millis;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = Config::resolve()?;
    tracing::info!(
        bind = %config.bind,
        data_dir = %config.data_dir.display(),
        "starting asm-daemon"
    );

    let db = Db::open(&config.db_path()).context("opening database")?;

    // A daemon restart means the in-process native PTYs are gone. Never
    // silently relaunch: reconcile any lingering live rows to `failed`.
    let orphaned = db.reconcile_orphans_on_startup(now_millis())?;
    if orphaned > 0 {
        tracing::warn!(
            "reconciled {orphaned} session(s) to `failed` after restart (native backend not recoverable in-process)"
        );
    }

    let registry = Arc::new(PluginRegistry::with_builtins());
    let backend = Arc::new(NativePtyBackend::new(db.events()));
    let worktree_root = config.data_dir.join("worktrees");
    let manager = Arc::new(SessionManager::new(db, registry, backend, worktree_root));

    let state = AppState {
        manager,
        config: Arc::new(config.clone()),
        scm: Arc::new(source_control::GitSourceControl),
        started_at: now_millis(),
    };
    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!("listening on http://{}", config.bind);

    axum::serve(listener, app)
        .await
        .context("http server error")?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_env("ASM_LOG").unwrap_or_else(|_| EnvFilter::new("info,asm_daemon=debug"));
    fmt().with_env_filter(filter).init();
}
