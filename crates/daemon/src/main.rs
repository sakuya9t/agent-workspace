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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use api::AppState;
use backend::asmux_client::AsmuxClient;
use backend::native::NativePtyBackend;
use backend::sidecar::SidecarBackend;
use backend::SessionBackend;
use config::{BackendKind, Config};
use db::Db;
use plugins::PluginRegistry;
use session_manager::SessionManager;
use util::now_millis;

#[tokio::main]
async fn main() -> Result<()> {
    // Subcommands run without the tracing subscriber so stdout stays clean.
    match std::env::args().nth(1).as_deref() {
        Some("token") | Some("enrollment-token") => return print_enrollment_token(),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some(other) if !other.starts_with('-') => {
            eprintln!("unknown command `{other}` — try `asm-daemon help`");
            std::process::exit(2);
        }
        _ => {}
    }

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
    tracing::info!("enrollment token for new devices: {enrollment_token}");
    tracing::info!("retrieve it anytime with `asm-daemon token`");
    if loopback_only {
        tracing::info!("bound to loopback: local clients are trusted; remote access via SSH port-forward needs no token");
    } else {
        tracing::warn!(
            "bound off-loopback ({}). Remote devices must enroll with the token above.",
            config.bind
        );
    }

    let registry = Arc::new(PluginRegistry::with_builtins());
    let worktree_root = config.data_dir.join("worktrees");

    // Select the session backend. The out-of-process holder (asmux) is what makes
    // sessions survive a daemon restart; the native in-process backend does not.
    let backend: Arc<dyn SessionBackend> = match config.backend {
        BackendKind::Native => {
            tracing::info!("session backend: native (in-process PTYs; do not survive restart)");
            Arc::new(NativePtyBackend::new(db.events()))
        }
        BackendKind::Sidecar => {
            ensure_asmux(&config).await?;
            let client = AsmuxClient::connect(&config.asmux_socket)
                .await
                .context("connecting to asmux holder")?;
            tracing::info!(
                socket = %config.asmux_socket.display(),
                instance_id = %client.instance_id,
                holder_pid = client.server_pid,
                "session backend: asmux holder (sessions survive daemon restart)"
            );
            Arc::new(SidecarBackend::new(client, db.events(), db.clone()))
        }
    };

    let manager = Arc::new(SessionManager::new(db, registry, backend, worktree_root));

    // Reconcile sessions left live by a previous run: adopt survivors from the
    // holder, or mark them failed/indeterminate. (Native marks them `failed`.)
    if let Err(e) = manager.startup_reconcile().await {
        tracing::error!("startup reconcile failed: {e:#}");
    }

    let state = AppState {
        manager: manager.clone(),
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
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    );

    // Race the server against a shutdown signal. We do NOT wait for open
    // connections to drain — a live terminal WebSocket would block that
    // indefinitely. Instead, on signal we kill every live child so no PTY (and,
    // for a future out-of-process/tmux backend, no sidecar) is ever leaked, then
    // exit; open sockets die with the process.
    tokio::select! {
        res = server => res.context("http server error")?,
        _ = shutdown_signal() => {
            let killed = manager.shutdown_all_live();
            tracing::info!("shutdown signal received; stopped {killed} live session(s)");
        }
    }
    Ok(())
}

/// Ensure the asmux holder is reachable, auto-spawning it (detached) if the
/// socket is dead and autospawn is enabled.
async fn ensure_asmux(config: &Config) -> Result<()> {
    use tokio::net::UnixStream;

    if UnixStream::connect(&config.asmux_socket).await.is_ok() {
        return Ok(());
    }
    if !config.asmux_autospawn {
        bail!(
            "asmux socket {} is unavailable and ASM_ASMUX_AUTOSPAWN=0",
            config.asmux_socket.display()
        );
    }

    let bin = resolve_asmux_bin(config);
    tracing::info!(bin = %bin.display(), "auto-spawning asmux holder");

    let mut cmd = std::process::Command::new(&bin);
    cmd.env("ASM_RUNTIME_DIR", &config.runtime_dir)
        .env("ASMUX_SOCK", &config.asmux_socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Detach into its own process group so a signal aimed at the daemon's group
    // (or the daemon dying) does not take the holder with it. Escaping a systemd
    // cgroup needs `systemd-run --user --scope` — see docs/deployment.md (M4).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .with_context(|| format!("spawning asmux binary at {}", bin.display()))?;

    // Wait for the socket to come up.
    for _ in 0..50 {
        if UnixStream::connect(&config.asmux_socket).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!(
        "asmux did not start listening at {}",
        config.asmux_socket.display()
    )
}

/// `ASM_ASMUX_BIN`, else a sibling of the daemon binary, else `asmux` on `PATH`.
fn resolve_asmux_bin(config: &Config) -> PathBuf {
    if let Some(p) = &config.asmux_bin {
        return p.clone();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(sib) = exe.parent().map(|d| d.join("asmux")) {
            if sib.exists() {
                return sib;
            }
        }
    }
    PathBuf::from("asmux")
}

/// Resolve when the process is asked to terminate (Ctrl-C / SIGINT, or SIGTERM
/// from a service manager). SIGTERM is Unix-only.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::warn!("could not install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_env("ASM_LOG").unwrap_or_else(|_| EnvFilter::new("info,asm_daemon=debug"));
    fmt().with_env_filter(filter).init();
}

/// `asm-daemon token` — print this host's enrollment token to stdout so a user
/// on the machine (or over SSH) can enroll another device.
fn print_enrollment_token() -> Result<()> {
    let config = Config::resolve()?;
    let db = Db::open(&config.db_path()).context("opening database")?;
    let (_, token) = db.get_or_create_identity(
        &auth::gen_server_id(),
        &auth::gen_enrollment_token(),
        now_millis(),
    )?;
    println!("{token}");
    Ok(())
}

fn print_help() {
    println!("asm-daemon — Agent Session Manager daemon\n");
    println!("USAGE:");
    println!("  asm-daemon           run the daemon");
    println!("  asm-daemon token     print this host's enrollment token");
    println!("  asm-daemon help      show this help\n");
    println!("ENV: ASM_BIND, ASM_DATA_DIR, ASM_CONFIG_DIR, ASM_RUNTIME_DIR, ASM_STATIC_DIR, ASM_LOG");
}
