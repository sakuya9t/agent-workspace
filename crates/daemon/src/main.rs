mod api;
mod auth;
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

    // Server identity + enrollment token (created once, persisted).
    let (server_id, enrollment_token) = db.get_or_create_identity(
        &auth::gen_server_id(),
        &auth::gen_enrollment_token(),
        now_millis(),
    )?;
    let loopback_only = config.bind.ip().is_loopback();
    tracing::info!(server_id = %server_id, "server identity ready");
    if loopback_only {
        tracing::info!("bound to loopback: local clients are trusted; remote access via SSH port-forward needs no token");
    } else {
        tracing::warn!(
            "bound off-loopback ({}). Remote devices must enroll. Enrollment token: {}",
            config.bind,
            enrollment_token
        );
    }

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

    // Connect-info exposes the peer address so auth can trust loopback.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
