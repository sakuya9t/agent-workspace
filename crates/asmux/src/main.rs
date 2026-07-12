//! asmux binary entrypoint.
//!
//! Resolves the runtime directory, binds the owner-only UDS, mints an
//! `instance_id`, and runs the server. The daemon (the *client*) connects to
//! `<runtime_dir>/asmux.sock`; asmux keeps holding PTYs across daemon restarts.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use asmux::registry::Registry;
use asmux::server::{serve_watched, ServerCtx};
use asmux::MEMORY_LIMIT_DEFAULT_BYTES;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ASM_LOG")
                .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let sock_path = resolve_socket_path()?;

    // `probe` is a read-only diagnostic for the service scripts: exit 0 iff a live
    // holder answers on this path. It lets them distinguish "alive" from "the pid
    // is alive but the socket is gone" — the orphan state that wedged the daemon
    // on 2026-07-12, which a pid check cannot see. Confirmed, not raw: scripts act
    // on this verdict (start.sh skips starting the holder on `Live`), so a fork-
    // window phantom here would wedge a boot. See `socket::probe_confirmed`.
    if std::env::args().nth(1).as_deref() == Some("probe") {
        let state = asmux::socket::probe_confirmed(&sock_path).await;
        println!("{state:?}");
        std::process::exit(if state == asmux::socket::SocketState::Live { 0 } else { 1 });
    }

    // Bind, refusing to displace a live holder (a test with a stale ASMUX_SOCK, a
    // stray `cargo run`). `bound_ino` lets the watchdog notice if our path is
    // later unlinked out from under us. See `socket::ensure_bindable`.
    let takeover = matches!(std::env::var("ASMUX_TAKEOVER").as_deref(), Ok("1"));
    let (listener, bound_ino) = asmux::socket::bind(&sock_path, takeover).await?;

    let memory_limit = std::env::var("ASMUX_MEMORY_LIMIT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(MEMORY_LIMIT_DEFAULT_BYTES);

    let instance_id = uuid::Uuid::new_v4().to_string();
    let started_at = now_ms();
    let server_pid = std::process::id() as i32;

    let registry = Arc::new(Registry::new(instance_id.clone(), started_at, memory_limit));
    let ctx = ServerCtx::new(registry, server_pid, String::new());

    tracing::info!(
        socket = %sock_path.display(),
        instance_id = %instance_id,
        pid = server_pid,
        memory_limit_mib = memory_limit / (1024 * 1024),
        "asmux listening"
    );

    let cleanup_path = sock_path.clone();
    tokio::select! {
        _ = serve_watched(listener, bound_ino, sock_path, ctx) => {}
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received");
        }
    }
    let _ = std::fs::remove_file(&cleanup_path);
    Ok(())
}

/// Complete on SIGINT or SIGTERM. SIGTERM is the systemd/`docker stop` signal,
/// so asmux must handle it (not just Ctrl-C) to unlink its socket on exit.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// `ASMUX_SOCK` (explicit socket) overrides; else `<runtime_dir>/asmux.sock`
/// where runtime_dir is `ASM_RUNTIME_DIR`, `ASMUX_RUNTIME_DIR`, or a temp dir.
fn resolve_socket_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ASMUX_SOCK") {
        return Ok(PathBuf::from(p));
    }
    let runtime_dir = std::env::var("ASM_RUNTIME_DIR")
        .or_else(|_| std::env::var("ASMUX_RUNTIME_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("asm"));
    Ok(runtime_dir.join("asmux.sock"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
