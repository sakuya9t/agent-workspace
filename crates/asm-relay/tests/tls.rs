//! End-to-end test of the *encrypted* relay hop.
//!
//! The plaintext path is covered by `e2e.rs`; this file covers the one that
//! actually ships. Both hops the design cares about are exercised at once:
//!
//! - **node → relay**: the agent dials `wss://` and registers. Until the TLS
//!   feature was enabled on `tokio-tungstenite` this failed outright, which is
//!   why a TLS-terminating proxy in front of the relay could never have been an
//!   ops-only change.
//! - **client → relay**: a client reaches the node over HTTPS *and* over WSS —
//!   the terminal-stream path — through `/n/<node_id>`.
//!
//! The relay presents a throwaway self-signed cert, so both clients are handed
//! it as a trust anchor; a real deployment uses an ACME cert and passes `None`.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use asm_relay::agent::{run as run_agent, AgentConfig, ResolvedDownstream};
use asm_relay::server::{serve, Registry};
use asm_relay::tls;
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as TMessage;
use tokio_tungstenite::Connector;

const KEY: &str = "test-relay-key";

/// A self-signed cert for `localhost`, as PEM (cert, key).
fn self_signed() -> (Vec<u8>, Vec<u8>) {
    let c = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    (
        c.cert.pem().into_bytes(),
        c.key_pair.serialize_pem().into_bytes(),
    )
}

/// A stand-in daemon behind the relay: one HTTP route, one WebSocket echo.
async fn spawn_fake_node() -> SocketAddr {
    let app = Router::new()
        .route("/hello", get(|| async { "hello from node" }))
        .route("/echo", get(echo_ws));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn echo_ws(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|mut socket: WebSocket| async move {
        while let Some(Ok(AxumWsMessage::Text(t))) = socket.recv().await {
            if socket.send(AxumWsMessage::Text(t)).await.is_err() {
                break;
            }
        }
    })
}

/// Blocking HTTPS GET that trusts the relay's self-signed cert.
async fn https_get(url: String, ca: Vec<u8>) -> (u16, String) {
    tokio::task::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .tls_config(tls::client_config(Some(&ca)).unwrap())
            .build();
        match agent.get(&url).call() {
            Ok(resp) => (resp.status(), resp.into_string().unwrap_or_default()),
            Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap_or_default()),
            Err(_) => (0, String::new()),
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn relayed_traffic_is_encrypted_end_to_end() {
    let (cert_pem, key_pem) = self_signed();
    let mut keys = HashSet::new();
    keys.insert(KEY.to_string());

    // The relay, serving TLS on an ephemeral loopback port. `serve` is the same
    // accept path the binary runs, so the handshake under test is the real one.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_tls = tls::server_config_pem(&cert_pem, &key_pem).unwrap();
    tokio::spawn(async move {
        serve(
            listener,
            Some(server_tls),
            Arc::new(Registry::new(keys)),
            true,
        )
        .await
        .unwrap();
    });

    // The NAT'd node: dials the relay outbound over WSS, trusting our CA.
    let node_http = spawn_fake_node().await;
    let node_id = "tls-node".to_string();
    let (_ds_tx, ds_rx) = tokio::sync::watch::channel(Vec::<ResolvedDownstream>::new());
    tokio::spawn(run_agent(AgentConfig {
        relay_url: format!("wss://localhost:{port}"),
        relay_key: KEY.to_string(),
        node_id: node_id.clone(),
        label: "tls-fake-node".to_string(),
        local_target: node_http,
        downstreams: ds_rx,
        relay_ca: Some(cert_pem.clone()),
    }));

    // The agent registered => it completed a wss:// handshake and a hello.
    let mut online = false;
    for _ in 0..100 {
        let (_, body) = https_get(
            format!("https://localhost:{port}/nodes?relay_key={KEY}"),
            cert_pem.clone(),
        )
        .await;
        if body.contains(&node_id) && body.contains("\"online\":true") {
            online = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(online, "node never registered over wss://");

    // (1) HTTPS through the tunnel, and HSTS on the way back out.
    let (status, body) = https_get(
        format!("https://localhost:{port}/n/{node_id}/hello?relay_key={KEY}"),
        cert_pem.clone(),
    )
    .await;
    assert_eq!(status, 200, "relayed HTTPS GET");
    assert!(body.contains("hello from node"), "relayed body: {body:?}");

    // (2) WSS through the tunnel — the terminal-stream path, which is the whole
    // point: a data stream survives TLS on both hops (client→relay, relay→node).
    let ws_url = format!("wss://localhost:{port}/n/{node_id}/echo?relay_key={KEY}");
    let connector = Connector::Rustls(tls::client_config(Some(&cert_pem)).unwrap());
    let (mut ws, _resp) =
        tokio_tungstenite::connect_async_tls_with_config(&ws_url, None, false, Some(connector))
            .await
            .expect("wss connect through relay");
    ws.send(TMessage::Text("ping-123".into())).await.unwrap();
    match ws.next().await.expect("ws reply").expect("ws reply ok") {
        TMessage::Text(t) => assert_eq!(t.as_str(), "ping-123", "ws echo over TLS"),
        other => panic!("unexpected ws reply: {other:?}"),
    }

    // (3) A plaintext client on the TLS port is refused, not downgraded: there
    // is no cleartext listener to fall back to.
    let plain = tokio::task::spawn_blocking(move || {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .build()
            .get(&format!("http://localhost:{port}/nodes?relay_key={KEY}"))
            .call()
            .is_ok()
    })
    .await
    .unwrap();
    assert!(
        !plain,
        "plaintext HTTP must not succeed against a TLS relay"
    );
}

/// An untrusted certificate must fail the dial rather than be waved through.
/// This is what stops a wss:// URL from being a false sense of security.
#[tokio::test]
async fn untrusted_relay_cert_is_refused() {
    let (cert_pem, key_pem) = self_signed();
    let mut keys = HashSet::new();
    keys.insert(KEY.to_string());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_tls = tls::server_config_pem(&cert_pem, &key_pem).unwrap();
    tokio::spawn(async move {
        serve(
            listener,
            Some(server_tls),
            Arc::new(Registry::new(keys)),
            true,
        )
        .await
        .unwrap();
    });

    // Public web PKI only — which does not vouch for our self-signed cert.
    let connector = Connector::Rustls(tls::client_config(None).unwrap());
    let result = tokio_tungstenite::connect_async_tls_with_config(
        &format!("wss://localhost:{port}/register?relay_key={KEY}"),
        None,
        false,
        Some(connector),
    )
    .await;
    assert!(
        result.is_err(),
        "a relay cert signed by nobody we trust must not be accepted"
    );
}
