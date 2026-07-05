#![forbid(unsafe_code)]

//! The standalone `asm-relay` server binary.
//!
//! Env:
//! - `ASM_RELAY_BIND` — listen address (default `127.0.0.1:4700`).
//! - `ASM_RELAY_KEYS` — comma-separated accepted relay access keys (required;
//!   the relay is not an open proxy).
//! - `ASM_RELAY_LOG` — tracing filter (default `info`).

use std::collections::HashSet;
use std::net::SocketAddr;

use anyhow::{bail, Context, Result};
use tracing_subscriber::EnvFilter;

use asm_relay::server::{run, RelayConfig};

#[tokio::main]
async fn main() -> Result<()> {
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

    run(RelayConfig { bind, keys }).await
}
