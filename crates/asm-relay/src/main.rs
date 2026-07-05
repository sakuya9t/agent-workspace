#![forbid(unsafe_code)]

//! The standalone `asm-relay` server binary.
//!
//! R1 lands incrementally: this stage binds the process and serves an (empty)
//! discovery endpoint so the crate is runnable end to end as it grows. The node
//! registry, the `/register` WSS + yamux control stream, and the `/n/<id>`
//! opaque proxy arrive in the following increments.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::{routing::get, Json, Router};
use tracing_subscriber::EnvFilter;

use asm_relay::protocol::NodesResponse;

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

    let app = Router::new().route("/nodes", get(nodes));

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!("asm-relay listening on http://{bind}");
    axum::serve(listener, app).await.context("relay server error")?;
    Ok(())
}

async fn nodes() -> Json<NodesResponse> {
    Json(NodesResponse { nodes: Vec::new() })
}
