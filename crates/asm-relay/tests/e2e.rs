//! In-process end-to-end test of the relay's dial-out path.
//!
//! Topology, all on loopback: a **fake node** (an axum server standing in for a
//! NAT'd daemon) + the relay **agent** dialing out to a relay **server**. A
//! blocking HTTP client (`ureq`) and a WebSocket client (`tokio-tungstenite`)
//! act as the user's client `A`, reaching the node only through `/n/<id>`.
//!
//! This proves the capability the R-track exists for: a private host reachable
//! solely by dialing out is fully controllable — HTTP and WebSocket — from a
//! client that can only reach the relay.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use asm_relay::agent::{run as run_agent, AgentConfig};
use asm_relay::server::{router, Registry};
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as TMessage;

const KEY: &str = "test-relay-key";

/// Spawn the relay on an ephemeral loopback port; return its address.
async fn spawn_relay(keys: HashSet<String>) -> SocketAddr {
    let reg = Arc::new(Registry::new(keys));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router(reg).into_make_service())
            .await
            .unwrap();
    });
    addr
}

/// A stand-in daemon: plain HTTP routes plus a WebSocket echo.
async fn spawn_fake_node() -> SocketAddr {
    let app = Router::new()
        .route("/hello", get(|| async { "hello from node" }))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(800)).await;
                "slow hello"
            }),
        )
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
        while let Some(Ok(msg)) = socket.recv().await {
            match msg {
                AxumWsMessage::Text(t) => {
                    if socket.send(AxumWsMessage::Text(t)).await.is_err() {
                        break;
                    }
                }
                AxumWsMessage::Binary(b) => {
                    if socket.send(AxumWsMessage::Binary(b)).await.is_err() {
                        break;
                    }
                }
                AxumWsMessage::Close(_) => break,
                _ => {}
            }
        }
    })
}

/// Blocking HTTP GET on its own agent (a fresh pool per call, so concurrent
/// calls are genuinely independent connections).
async fn http_get(url: String) -> (u16, String) {
    tokio::task::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .build();
        match agent.get(&url).call() {
            Ok(resp) => {
                let status = resp.status();
                (status, resp.into_string().unwrap_or_default())
            }
            Err(ureq::Error::Status(code, resp)) => {
                (code, resp.into_string().unwrap_or_default())
            }
            Err(_) => (0, String::new()),
        }
    })
    .await
    .unwrap()
}

async fn wait_online(relay: SocketAddr, node_id: &str) {
    for _ in 0..100 {
        let (_, body) = http_get(format!("http://{relay}/nodes?relay_key={KEY}")).await;
        if body.contains(node_id) && body.contains("\"online\":true") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("node {node_id} never came online");
}

#[tokio::test]
async fn relayed_nat_host_is_controllable_end_to_end() {
    let mut keys = HashSet::new();
    keys.insert(KEY.to_string());
    let relay = spawn_relay(keys).await;
    let node_http = spawn_fake_node().await;
    let node_id = "node-1".to_string();

    let agent = tokio::spawn(run_agent(AgentConfig {
        relay_url: format!("ws://{relay}"),
        relay_key: KEY.to_string(),
        node_id: node_id.clone(),
        label: "fake-node".to_string(),
        local_target: node_http,
        downstreams: vec![],
    }));

    wait_online(relay, &node_id).await;

    // (1) HTTP round-trip through the tunnel.
    let (status, body) = http_get(format!("http://{relay}/n/{node_id}/hello?relay_key={KEY}")).await;
    assert_eq!(status, 200, "relayed GET status");
    assert!(body.contains("hello from node"), "relayed GET body: {body:?}");

    // (2) WebSocket echo through the tunnel (the terminal-stream path).
    {
        let ws_url = format!("ws://{relay}/n/{node_id}/echo?relay_key={KEY}");
        let (mut ws, _resp) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("ws connect through relay");
        ws.send(TMessage::Text("ping-123".into())).await.unwrap();
        let reply = ws.next().await.expect("ws reply").expect("ws reply ok");
        match reply {
            TMessage::Text(t) => assert_eq!(t.as_str(), "ping-123", "ws echo"),
            other => panic!("unexpected ws reply: {other:?}"),
        }
    }

    // (3) Relay-key auth: wrong key and missing key are both 401.
    let (status, _) =
        http_get(format!("http://{relay}/n/{node_id}/hello?relay_key=WRONG")).await;
    assert_eq!(status, 401, "wrong relay key");
    let (status, _) = http_get(format!("http://{relay}/n/{node_id}/hello")).await;
    assert_eq!(status, 401, "missing relay key");

    // (4) Unknown node is 404.
    let (status, _) = http_get(format!("http://{relay}/n/ghost/hello?relay_key={KEY}")).await;
    assert_eq!(status, 404, "unknown node");

    // (5) A stalled stream must not block a concurrent one (dial-out isolation).
    let slow = tokio::spawn(http_get(format!(
        "http://{relay}/n/{node_id}/slow?relay_key={KEY}"
    )));
    // Give the slow request a beat to occupy its own data stream.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let t0 = Instant::now();
    let (status, body) =
        http_get(format!("http://{relay}/n/{node_id}/hello?relay_key={KEY}")).await;
    let fast_elapsed = t0.elapsed();
    assert_eq!(status, 200);
    assert!(body.contains("hello from node"));
    assert!(
        fast_elapsed < Duration::from_millis(500),
        "fast request blocked by the slow one: {fast_elapsed:?}"
    );
    let _ = slow.await;

    // (6) A dropped node goes offline and is 502, not a hang or a fake 200.
    agent.abort();
    let mut offline = false;
    for _ in 0..100 {
        let (_, body) = http_get(format!("http://{relay}/nodes?relay_key={KEY}")).await;
        if !body.contains("\"online\":true") {
            offline = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(offline, "node never went offline after the agent dropped");
    let (status, _) =
        http_get(format!("http://{relay}/n/{node_id}/hello?relay_key={KEY}")).await;
    assert_eq!(status, 502, "offline node should be 502");
}
