//! asmux binary entrypoint.
//!
//! Resolves the runtime directory, binds the owner-only UDS, mints an
//! `instance_id`, and runs the server. The daemon (the *client*) connects to
//! `<runtime_dir>/asmux.sock`; asmux keeps holding PTYs across daemon restarts.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::net::UnixListener;

use asmux::registry::Registry;
use asmux::server::{serve, ServerCtx};
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
    if let Some(dir) = sock_path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create runtime dir {dir:?}"))?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0700 {dir:?}"))?;
    }

    // Remove a stale socket. NOTE (M4): before removing we should probe for a
    // live holder and refuse to clobber it; for M1 single-user dev this is a
    // plain unlink.
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)
        .with_context(|| format!("bind UDS at {sock_path:?}"))?;
    std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {sock_path:?}"))?;

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
        _ = serve(listener, ctx) => {}
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
