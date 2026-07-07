//! The relay server: registry, control/data WebSocket endpoints, discovery, and
//! the opaque `/n/<node_id>/*` proxy.
//!
//! The relay authenticates only the *relay access key*; the daemon device token
//! rides through untouched in the `Authorization` header and is validated end to
//! end by the target daemon. The relay never parses the daemon API — a proxied
//! request is forwarded byte-for-byte over a dial-back data stream.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};
use tower_http::cors::CorsLayer;

use crate::protocol::{
    DownstreamInfo, NodeEntry, NodeKind, NodeMsg, NodesResponse, RelayError, RelayMsg,
    DATA_STREAM_QUERY, OFFLINE_AFTER, PROTO_VERSION, RELAY_KEY_HEADER, RELAY_KEY_QUERY,
};
use crate::transport::WsByteStream;

/// How long a proxied request waits for the node to dial its data stream back
/// before the relay gives up with a 502.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// A dial-back data stream, presented to the proxy as a byte duplex.
type DataConn = WsByteStream<WebSocket>;

// ------------------------------------------------------------------ config

pub struct RelayConfig {
    pub bind: SocketAddr,
    pub keys: HashSet<String>,
}

pub async fn run(config: RelayConfig) -> Result<()> {
    let reg = Arc::new(Registry::new(config.keys));
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!("asm-relay listening on http://{}", config.bind);
    axum::serve(listener, router(reg))
        .await
        .context("relay server error")
}

/// Build the relay router. Exposed so tests can serve it on an ephemeral port.
pub fn router(reg: Arc<Registry>) -> Router {
    Router::new()
        .route("/nodes", get(nodes))
        .route("/register", get(register))
        .route("/data", get(data))
        .route("/n/:node_id/*rest", any(proxy))
        .layer(middleware::from_fn_with_state(reg.clone(), require_relay_key))
        .layer(CorsLayer::permissive())
        .with_state(reg)
}

// ------------------------------------------------------------------ registry

struct NodeState {
    label: String,
    downstreams: Vec<DownstreamInfo>,
    last_seen: SystemTime,
    connected: bool,
    /// Bumped on every (re)registration; a stale control loop only mutates its
    /// own entry when the generation still matches (single-attacher takeover).
    generation: u64,
    ctrl_tx: mpsc::UnboundedSender<RelayMsg>,
}

impl NodeState {
    fn online(&self) -> bool {
        self.connected
            && self
                .last_seen
                .elapsed()
                .map(|d| d < OFFLINE_AFTER)
                .unwrap_or(false)
    }

    fn kind(&self) -> NodeKind {
        if self.downstreams.is_empty() {
            NodeKind::Leaf
        } else {
            NodeKind::Gateway
        }
    }
}

/// Where a `/n/<id>` request routes: through which connected node (`via`) and
/// which `target` that node should dial locally.
struct Route {
    via: String,
    target: String,
    is_downstream: bool,
}

pub struct Registry {
    keys: HashSet<String>,
    nodes: Mutex<HashMap<String, NodeState>>,
    pending: Mutex<HashMap<String, oneshot::Sender<DataConn>>>,
    next_gen: AtomicU64,
}

impl Registry {
    pub fn new(keys: HashSet<String>) -> Self {
        Self {
            keys,
            nodes: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            next_gen: AtomicU64::new(1),
        }
    }

    fn key_ok(&self, headers: &HeaderMap, uri: &Uri) -> bool {
        // Query param wins if present (browsers cannot set headers).
        if let Some(q) = uri.query() {
            if let Some(v) = find_query(q, RELAY_KEY_QUERY) {
                return self.keys.contains(v);
            }
        }
        if let Some(h) = headers.get(RELAY_KEY_HEADER) {
            if let Ok(s) = h.to_str() {
                return self.keys.contains(s);
            }
        }
        false
    }

    /// Register (or take over) a node; returns this registration's generation.
    fn register(
        &self,
        node_id: &str,
        label: String,
        downstreams: Vec<DownstreamInfo>,
        ctrl_tx: mpsc::UnboundedSender<RelayMsg>,
    ) -> u64 {
        let generation = self.next_gen.fetch_add(1, Ordering::Relaxed);
        let mut nodes = self.nodes.lock();
        if nodes.contains_key(node_id) {
            tracing::info!(node = %node_id, "node re-registered; superseding prior connection");
        }
        nodes.insert(
            node_id.to_string(),
            NodeState {
                label,
                downstreams,
                last_seen: SystemTime::now(),
                connected: true,
                generation,
                ctrl_tx,
            },
        );
        generation
    }

    fn touch(&self, node_id: &str, generation: u64) {
        if let Some(s) = self.nodes.lock().get_mut(node_id) {
            if s.generation == generation {
                s.last_seen = SystemTime::now();
            }
        }
    }

    fn set_downstreams(&self, node_id: &str, generation: u64, downstreams: Vec<DownstreamInfo>) {
        if let Some(s) = self.nodes.lock().get_mut(node_id) {
            if s.generation == generation {
                s.downstreams = downstreams;
                s.last_seen = SystemTime::now();
            }
        }
    }

    /// Mark a node disconnected, but only if this is still its live generation
    /// (a superseding registration must not be clobbered by the old loop).
    fn disconnect(&self, node_id: &str, generation: u64) {
        if let Some(s) = self.nodes.lock().get_mut(node_id) {
            if s.generation == generation {
                s.connected = false;
            }
        }
    }

    /// Resolve a `/n/<id>` target to the connection that serves it.
    fn route(&self, node_id: &str) -> Option<Route> {
        let nodes = self.nodes.lock();
        if nodes.contains_key(node_id) {
            return Some(Route {
                via: node_id.to_string(),
                target: node_id.to_string(),
                is_downstream: false,
            });
        }
        // Otherwise it may be an advertised downstream of some gateway.
        for (gw_id, st) in nodes.iter() {
            if st.downstreams.iter().any(|d| d.node_id == node_id) {
                return Some(Route {
                    via: gw_id.clone(),
                    target: node_id.to_string(),
                    is_downstream: true,
                });
            }
        }
        None
    }

    /// The control sender for a node, iff it is currently online.
    fn ctrl_if_online(&self, node_id: &str) -> Option<mpsc::UnboundedSender<RelayMsg>> {
        let nodes = self.nodes.lock();
        nodes
            .get(node_id)
            .filter(|s| s.online())
            .map(|s| s.ctrl_tx.clone())
    }

    fn take_pending(&self, stream_id: &str) -> Option<oneshot::Sender<DataConn>> {
        self.pending.lock().remove(stream_id)
    }

    fn snapshot(&self) -> Vec<NodeEntry> {
        let nodes = self.nodes.lock();
        let mut out = Vec::new();
        for (id, st) in nodes.iter() {
            out.push(NodeEntry {
                node_id: id.clone(),
                label: st.label.clone(),
                kind: st.kind(),
                via: None,
                online: st.online(),
                last_seen: ms_string(st.last_seen),
            });
            // Surface each advertised downstream as its own entry, via this node.
            for d in &st.downstreams {
                out.push(NodeEntry {
                    node_id: d.node_id.clone(),
                    label: d.label.clone(),
                    kind: NodeKind::Leaf,
                    via: Some(id.clone()),
                    online: st.online() && d.reachable,
                    last_seen: ms_string(st.last_seen),
                });
            }
        }
        out
    }
}

// ------------------------------------------------------------------ auth

async fn require_relay_key(
    State(reg): State<Arc<Registry>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // CORS preflight carries no key and is normally short-circuited by the CORS
    // layer; pass any OPTIONS through untouched so a cross-origin browser client
    // is never blocked before the key-bearing request.
    if req.method() == axum::http::Method::OPTIONS {
        return next.run(req).await;
    }
    if reg.key_ok(req.headers(), req.uri()) {
        next.run(req).await
    } else {
        err_response(RelayError::Unauthorized)
    }
}

// ------------------------------------------------------------------ /nodes

async fn nodes(State(reg): State<Arc<Registry>>) -> Json<NodesResponse> {
    Json(NodesResponse {
        nodes: reg.snapshot(),
    })
}

// ------------------------------------------------------------------ /register

async fn register(State(reg): State<Arc<Registry>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| register_conn(socket, reg))
}

async fn register_conn(socket: WebSocket, reg: Arc<Registry>) {
    let (mut sink, mut stream) = socket.split();

    // First control frame must be a valid, compatible Hello.
    let hello = match next_text(&mut stream).await {
        Some(t) => match serde_json::from_str::<NodeMsg>(&t) {
            Ok(NodeMsg::Hello {
                proto,
                node_id,
                label,
                downstreams,
            }) if proto == PROTO_VERSION => (node_id, label, downstreams),
            _ => {
                let _ = send_error(&mut sink, "bad_hello", "expected a compatible hello").await;
                return;
            }
        },
        None => return,
    };
    let (node_id, label, downstreams) = hello;

    // One writer task owns the sink; pongs and Opens funnel through ctrl_tx.
    let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded_channel::<RelayMsg>();
    let writer = tokio::spawn(async move {
        while let Some(m) = ctrl_rx.recv().await {
            let text = match serde_json::to_string(&m) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if sink.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });

    let generation = reg.register(&node_id, label, downstreams, ctrl_tx.clone());
    tracing::info!(node = %node_id, generation, "node registered");

    // Pump inbound control frames until the socket closes.
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Text(t)) => match serde_json::from_str::<NodeMsg>(&t) {
                Ok(NodeMsg::Ping { seq }) => {
                    reg.touch(&node_id, generation);
                    let _ = ctrl_tx.send(RelayMsg::Pong { seq });
                }
                Ok(NodeMsg::Downstreams { downstreams }) => {
                    reg.set_downstreams(&node_id, generation, downstreams);
                }
                Ok(NodeMsg::Hello { .. }) => {} // ignore a repeated hello
                Err(e) => tracing::warn!(node = %node_id, "undecodable control frame: {e}"),
            },
            Ok(Message::Close(_)) => break,
            Ok(_) => {} // binary/ping/pong on the control stream: ignore
            Err(e) => {
                tracing::warn!(node = %node_id, "control stream error: {e}");
                break;
            }
        }
    }

    reg.disconnect(&node_id, generation);
    writer.abort();
    tracing::info!(node = %node_id, "node control stream ended");
}

// ------------------------------------------------------------------ /data

async fn data(
    State(reg): State<Arc<Registry>>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    let stream_id = params.get(DATA_STREAM_QUERY).cloned();
    ws.on_upgrade(move |socket| async move {
        let Some(stream_id) = stream_id else {
            return; // no correlation id — drop
        };
        match reg.take_pending(&stream_id) {
            Some(tx) => {
                // Hand the byte duplex to the waiting proxy. If it already timed
                // out (rx dropped), the send fails and the socket is dropped.
                let _ = tx.send(WsByteStream::new(socket));
            }
            None => {
                tracing::warn!(%stream_id, "data stream with no pending request; dropping");
            }
        }
    })
}

// ------------------------------------------------------------------ /n/<id>/*

async fn proxy(
    State(reg): State<Arc<Registry>>,
    Path((node_id, _rest)): Path<(String, String)>,
    req: Request<Body>,
) -> Response {
    let Some(route) = reg.route(&node_id) else {
        return err_response(RelayError::UnknownNode);
    };
    let Some(ctrl_tx) = reg.ctrl_if_online(&route.via) else {
        return err_response(RelayError::NodeOffline);
    };
    // The error to surface when the routed node fails to produce a stream.
    let unreachable = if route.is_downstream {
        RelayError::DownstreamUnreachable
    } else {
        RelayError::NodeOffline
    };

    // Ask the node to dial a data stream back, and wait for it.
    let stream_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<DataConn>();
    reg.pending.lock().insert(stream_id.clone(), tx);

    if ctrl_tx
        .send(RelayMsg::Open {
            stream_id: stream_id.clone(),
            target: route.target,
        })
        .is_err()
    {
        reg.pending.lock().remove(&stream_id);
        return err_response(RelayError::NodeOffline);
    }

    let data = match tokio::time::timeout(OPEN_TIMEOUT, rx).await {
        Ok(Ok(conn)) => conn,
        _ => {
            reg.pending.lock().remove(&stream_id);
            return err_response(unreachable);
        }
    };

    match forward(req, data).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(node = %node_id, "proxy forward failed: {e:#}");
            err_response(unreachable)
        }
    }
}

/// Forward one client request over the data stream via an HTTP/1.1 client,
/// splicing the connection through on a WebSocket upgrade.
async fn forward(mut req: Request<Body>, data: DataConn) -> Result<Response> {
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(data))
        .await
        .context("http1 handshake over data stream")?;
    // Drive the connection (with upgrade support) in the background.
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let is_upgrade = is_websocket_upgrade(req.headers());
    let client_upgrade = if is_upgrade {
        Some(hyper::upgrade::on(&mut req))
    } else {
        None
    };

    let (mut parts, body) = req.into_parts();
    parts.uri = forward_uri(&parts.uri);
    let fwd = Request::from_parts(parts, body);

    let mut resp = sender
        .send_request(fwd)
        .await
        .context("forwarding request to node")?;

    if resp.status() == StatusCode::SWITCHING_PROTOCOLS {
        // Build the 101 to return to the client, mirroring the node's headers.
        let mut builder = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
        for (k, v) in resp.headers() {
            builder = builder.header(k, v);
        }
        let client_response = builder
            .body(Body::empty())
            .context("building 101 response")?;

        let node_upgrade = hyper::upgrade::on(&mut resp);
        tokio::spawn(async move {
            let node_io = match node_upgrade.await {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!("node upgrade failed: {e}");
                    return;
                }
            };
            let client_io = match client_upgrade {
                Some(fut) => match fut.await {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!("client upgrade failed: {e}");
                        return;
                    }
                },
                None => return,
            };
            let mut a = TokioIo::new(node_io);
            let mut b = TokioIo::new(client_io);
            let _ = tokio::io::copy_bidirectional(&mut a, &mut b).await;
        });

        Ok(client_response)
    } else {
        Ok(resp.map(Body::new))
    }
}

// ------------------------------------------------------------------ helpers

/// Read frames until the first Text, returning its payload; `None` on close/eof.
async fn next_text(stream: &mut futures::stream::SplitStream<WebSocket>) -> Option<String> {
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Text(t)) => return Some(t),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => continue,
        }
    }
    None
}

async fn send_error(
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    code: &str,
    message: &str,
) -> Result<()> {
    let m = RelayMsg::Error {
        code: code.to_string(),
        message: message.to_string(),
    };
    sink.send(Message::Text(serde_json::to_string(&m)?)).await?;
    let _ = sink.close().await;
    Ok(())
}

fn err_response(e: RelayError) -> Response {
    let status = StatusCode::from_u16(e.status()).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, Json(json!({ "error": e.code() }))).into_response()
}

fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    let has_upgrade_conn = headers
        .get(axum::http::header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    let is_ws = headers
        .get(axum::http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    has_upgrade_conn && is_ws
}

/// Rewrite an incoming `/n/<id>/<rest>?<q>` URI to origin-form `/<rest>?<q'>` for
/// the node, stripping the relay key from the query. The daemon device token in
/// the `Authorization` header is untouched.
fn forward_uri(uri: &Uri) -> Uri {
    // path is "/n/<id>/<rest>"; drop the "/n/<id>" prefix (three leading slashes)
    let path = uri.path();
    let rest = strip_node_prefix(path);
    let filtered = uri.query().map(strip_relay_key).unwrap_or_default();
    let pq = if filtered.is_empty() {
        rest.to_string()
    } else {
        format!("{rest}?{filtered}")
    };
    pq.parse().unwrap_or_else(|_| Uri::from_static("/"))
}

/// `/n/<id>/api/x` -> `/api/x`; `/n/<id>` -> `/`.
fn strip_node_prefix(path: &str) -> &str {
    // Skip "/n/", then skip the node id up to the next '/'.
    let after_n = path.strip_prefix("/n/").unwrap_or(path);
    match after_n.find('/') {
        Some(i) => &after_n[i..],
        None => "/",
    }
}

fn strip_relay_key(query: &str) -> String {
    query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("");
            key != RELAY_KEY_QUERY
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn find_query<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut it = pair.splitn(2, '=');
        match (it.next(), it.next()) {
            (Some(k), Some(v)) if k == key => Some(v),
            _ => None,
        }
    })
}

fn ms_string(t: SystemTime) -> String {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
