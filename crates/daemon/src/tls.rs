//! HTTPS for the daemon's own listener.
//!
//! Without this the only encrypted ways to reach a daemon are the relay and an
//! SSH port-forward, so a client on the LAN talking to it directly
//! (`http://192.168.x.x:4600`) hands its device token — and every keystroke — to
//! anyone sniffing the network. With `ASM_TLS_CERT`/`ASM_TLS_KEY` set, that same
//! client uses `https://` and the direct path is encrypted like any other.
//!
//! Certificate parsing is [`asm_relay::tls`]'s, shared with the relay: one code
//! path for "read a PEM chain + key, refuse a mismatched pair".
//!
//! Two deliberate differences from the relay's TLS:
//!
//! - **No HSTS.** A daemon is usually reached by IP or a LAN name with a
//!   self-signed certificate, and HSTS makes the browser's certificate
//!   interstitial *non-bypassable* — the user could never click through, and
//!   turning TLS back off would lock them out of the host entirely.
//! - **`ConnectInfo` is inserted by hand.** `axum::serve` normally does this via
//!   `into_make_service_with_connect_info`, but this accept loop bypasses that,
//!   and `auth::require_auth` derives loopback trust from the peer address. If
//!   the extension were missing, `peer_is_loopback` would silently return false
//!   and every local client would start getting 401s.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::Router;
use hyper_util::rt::TokioIo;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;
use tower::Service;

/// How long an unauthenticated peer may take to complete a TLS handshake, and
/// how many may be doing so at once. A peer that connects and then goes silent
/// would otherwise hold a task and a file descriptor for as long as it liked —
/// and this listener is, by definition, the one exposed to the network.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING_HANDSHAKES: usize = 256;

/// Serve `app` over TLS on `listener` until the process ends.
pub async fn serve_https(listener: TcpListener, config: ServerConfig, app: Router) -> Result<()> {
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let pending = Arc::new(Semaphore::new(MAX_PENDING_HANDSHAKES));
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            // A per-connection accept error (fd exhaustion, a reset) must not
            // take the daemon down; the listener stays live.
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        // The permit covers the handshake only; established connections release
        // it, so live terminal streams never count against the bound.
        let permit = match Arc::clone(&pending).try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(%peer, "too many TLS handshakes in flight; dropping connection");
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let tls = match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
                Ok(Ok(s)) => s,
                // Where a plaintext client dialing the HTTPS port lands.
                Ok(Err(e)) => {
                    tracing::debug!(%peer, "TLS handshake failed: {e}");
                    return;
                }
                Err(_) => {
                    tracing::debug!(%peer, "TLS handshake timed out; dropping connection");
                    return;
                }
            };
            drop(permit);
            let svc = hyper::service::service_fn(move |mut req: Request<hyper::body::Incoming>| {
                req.extensions_mut().insert(ConnectInfo(peer));
                app.clone().call(req)
            });
            // `with_upgrades` is load-bearing: every terminal stream is a
            // WebSocket upgrade.
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(tls), svc)
                .with_upgrades()
                .await
            {
                tracing::debug!(%peer, "connection ended: {e}");
            }
        });
    }
}

/// Load the daemon's TLS config from PEM files.
pub fn server_config(cert: &std::path::Path, key: &std::path::Path) -> Result<ServerConfig> {
    asm_relay::tls::server_config(cert, key).context("loading the daemon's TLS certificate")
}
