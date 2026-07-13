#![forbid(unsafe_code)]

//! The standalone `asm-relay` server binary.
//!
//! Env:
//! - `ASM_RELAY_BIND` — listen address (default `127.0.0.1:4700`).
//! - `ASM_RELAY_KEYS` — comma-separated accepted relay access keys (required;
//!   the relay is not an open proxy).
//! - `ASM_RELAY_TLS_CERT` / `ASM_RELAY_TLS_KEY` — PEM certificate chain + key.
//!   Set both to serve HTTPS/WSS directly; the relay then speaks only TLS on
//!   its bind address. Leave both unset when a TLS-terminating reverse proxy
//!   sits in front, and set `ASM_RELAY_HSTS=1` instead.
//! - `ASM_RELAY_HSTS` — send `Strict-Transport-Security` (implied when serving
//!   TLS directly; set it for the proxy deployment).
//! - `ASM_RELAY_LOG` — tracing filter (default `info`).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use tracing_subscriber::EnvFilter;

use asm_relay::server::{run, RelayConfig, TlsPaths};

#[tokio::main]
async fn main() -> Result<()> {
    // `asm-relay check-tls CERT KEY` — load the TLS material and exit. Callers
    // (the service scripts) use this to find out whether a certificate is usable
    // *before* they stop a healthy relay to apply it: readable is not the same as
    // valid, and a mismatched key or a PEM full of the wrong thing would
    // otherwise turn a config typo into an outage. It runs the real load path,
    // not a lookalike.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("check-tls") {
        let (cert, key) = match (args.next(), args.next()) {
            (Some(c), Some(k)) => (PathBuf::from(c), PathBuf::from(k)),
            _ => bail!("usage: asm-relay check-tls <cert.pem> <key.pem>"),
        };
        asm_relay::tls::server_config(&cert, &key)?;
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("ASM_RELAY_LOG")
                .unwrap_or_else(|_| EnvFilter::new("info,asm_relay=debug")),
        )
        .init();

    let bind: SocketAddr = std::env::var("ASM_RELAY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:4700".to_string())
        .parse()
        .context("parsing ASM_RELAY_BIND")?;

    let keys: HashSet<String> = std::env::var("ASM_RELAY_KEYS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if keys.is_empty() {
        bail!("ASM_RELAY_KEYS is empty — set at least one relay access key");
    }

    // Half a TLS config is a misconfiguration, not a degraded mode: silently
    // falling back to plaintext is exactly the failure this is meant to prevent.
    let tls = match (
        env_path("ASM_RELAY_TLS_CERT"),
        env_path("ASM_RELAY_TLS_KEY"),
    ) {
        (Some(cert), Some(key)) => Some(TlsPaths { cert, key }),
        (None, None) => None,
        _ => bail!("set BOTH ASM_RELAY_TLS_CERT and ASM_RELAY_TLS_KEY, or neither"),
    };
    let hsts = matches!(std::env::var("ASM_RELAY_HSTS").as_deref(), Ok("1"));

    run(RelayConfig {
        bind,
        keys,
        tls,
        hsts,
    })
    .await
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}
