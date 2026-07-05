//! The frozen relay wire contract.
//!
//! This module is the single source of truth for everything that crosses the
//! relay boundary — auth header/param names, the control-stream messages a node
//! and the relay exchange, the `/nodes` discovery shape, and the client-facing
//! error bodies. It is shared by the relay server (`crate::server`) and the
//! node-side agent (`crate::agent`), and it mirrors
//! `docs/connectivity-execution-plan.md` → *Wire contract*.
//!
//! # Two connection kinds (dial-out-per-stream)
//!
//! A node holds one long-lived **control WSS** (`/register`). For each inbound
//! client connection the relay sends an [`RelayMsg::Open`] down the control
//! channel; the node dials a fresh outbound **data WSS**
//! (`/data?stream_id=…`), which the relay pairs with the waiting client and
//! splices. The node only ever connects *outbound*, so this works through NAT.
//! The stream's target travels in `Open`, so there is no in-stream preamble.
//!
//! Serialization is JSON: the control stream is JSON Lines (one object per
//! line); the HTTP surfaces are ordinary JSON bodies. Data streams carry opaque
//! bytes (the daemon's HTTP/1.1 or upgraded-WS traffic) — the relay never
//! parses them.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Bumped only on a breaking change to the control-stream framing. The relay
/// rejects a `hello` whose `proto` it does not accept.
pub const PROTO_VERSION: u32 = 1;

/// Relay access key — gates *use of the relay itself* (register, discover,
/// route). Independent from the per-node device token, which the relay never
/// inspects. Native nodes send the header; browsers (which cannot set WS
/// headers) send the query param; the query param wins if both are present.
pub const RELAY_KEY_HEADER: &str = "x-asm-relay-key";
pub const RELAY_KEY_QUERY: &str = "relay_key";

/// Query param carrying the correlation id on a dial-back data WSS
/// (`/data?stream_id=…`). The relay mints the id (unguessable UUID), hands it
/// to exactly one node over that node's authenticated control stream, and pairs
/// the returning data WSS back to the waiting client request.
pub const DATA_STREAM_QUERY: &str = "stream_id";

/// Relay HTTP paths the node dials.
pub const REGISTER_PATH: &str = "/register";
pub const DATA_PATH: &str = "/data";

/// The node opens the control stream first; every other yamux stream is a proxy
/// stream. Node → relay control messages are JSON Lines tagged by `t`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum NodeMsg {
    /// First line on the control stream. A `proto` the relay does not accept,
    /// or a `node_id` colliding with a *different* live connection, is refused
    /// with a `RelayMsg::Error` line followed by a WS close. The same `node_id`
    /// reconnecting supersedes the older registration (takeover).
    Hello {
        proto: u32,
        node_id: String,
        label: String,
        #[serde(default)]
        downstreams: Vec<DownstreamInfo>,
    },
    /// Replace-set update of this gateway's reachable downstreams, sent whenever
    /// a probe result changes.
    Downstreams { downstreams: Vec<DownstreamInfo> },
    /// Liveness. Sent every [`PING_INTERVAL`]; the relay answers with
    /// [`RelayMsg::Pong`]. JSON ping is the authoritative liveness signal — WS
    /// ping frames are not relied on.
    Ping { seq: u64 },
}

/// Relay → node control messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum RelayMsg {
    Pong { seq: u64 },
    /// Ask the node to dial back a data WSS for a waiting client connection.
    /// `target` is the node's own `node_id` (traffic for itself) or one of its
    /// advertised downstream `node_id`s. The node connects
    /// `/data?stream_id=<stream_id>`, then dials the matching local address and
    /// splices bytes.
    Open { stream_id: String, target: String },
    /// Terminal: the relay writes this then closes the connection (bad proto,
    /// bad key discovered post-upgrade, or node_id collision).
    Error { code: String, message: String },
}

/// A private-network target a gateway advertises. `node_id`/`label` are learned
/// by probing the downstream daemon's `/health`; `reachable` reflects the most
/// recent probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownstreamInfo {
    pub node_id: String,
    pub label: String,
    pub reachable: bool,
}

/// Node liveness classification in the relay registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Registers itself only.
    Leaf,
    /// Registers itself and advertises downstreams it reaches inward.
    Gateway,
}

/// One row of `GET /nodes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEntry {
    pub node_id: String,
    pub label: String,
    pub kind: NodeKind,
    /// For an advertised downstream, the `node_id` of the gateway that reaches
    /// it; `None` for a directly-registered node.
    pub via: Option<String>,
    pub online: bool,
    /// RFC3339 timestamp of the last control-stream traffic (or last probe, for
    /// a downstream). Advisory only.
    pub last_seen: String,
}

/// `GET /nodes` response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodesResponse {
    pub nodes: Vec<NodeEntry>,
}

/// Client-facing relay error. The `code` strings are frozen — the client keys
/// distinct UI states off them (see connectivity-execution-plan.md → Client
/// contract → Failure states).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayError {
    /// 401 — bad or missing relay key.
    Unauthorized,
    /// 404 — `node_id` never registered.
    UnknownNode,
    /// 502 — node registered, but its connection is currently down.
    NodeOffline,
    /// 502 — gateway is up, but its probe of the downstream is failing.
    DownstreamUnreachable,
}

impl RelayError {
    /// The frozen machine-readable code placed in `{"error": <code>}`.
    pub const fn code(self) -> &'static str {
        match self {
            RelayError::Unauthorized => "relay_unauthorized",
            RelayError::UnknownNode => "unknown_node",
            RelayError::NodeOffline => "node_offline",
            RelayError::DownstreamUnreachable => "downstream_unreachable",
        }
    }

    /// HTTP status as a bare `u16` (kept dependency-free; the server maps it to
    /// its `StatusCode`).
    pub const fn status(self) -> u16 {
        match self {
            RelayError::Unauthorized => 401,
            RelayError::UnknownNode => 404,
            RelayError::NodeOffline | RelayError::DownstreamUnreachable => 502,
        }
    }
}

// ---- Liveness / reconnect timings (see plan → Registration) ----

/// Node → relay ping cadence.
pub const PING_INTERVAL: Duration = Duration::from_secs(15);
/// The relay marks a node offline after this long with no control traffic; the
/// node treats a missing pong the same way and reconnects.
pub const OFFLINE_AFTER: Duration = Duration::from_secs(45);

/// Node reconnect backoff (exponential, jittered). The stable-reset threshold
/// is how long a connection must hold before the backoff resets to the floor.
pub const BACKOFF_MIN: Duration = Duration::from_secs(1);
pub const BACKOFF_MAX: Duration = Duration::from_secs(60);
pub const BACKOFF_STABLE_RESET: Duration = Duration::from_secs(60);
/// Fractional jitter applied to each backoff delay (±20%).
pub const BACKOFF_JITTER: f64 = 0.20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrips_as_tagged_json_line() {
        let msg = NodeMsg::Hello {
            proto: PROTO_VERSION,
            node_id: "n1".into(),
            label: "host-a".into(),
            downstreams: vec![DownstreamInfo {
                node_id: "d1".into(),
                label: "internal".into(),
                reachable: true,
            }],
        };
        let line = serde_json::to_string(&msg).expect("serialize");
        assert!(line.contains("\"t\":\"hello\""));
        assert!(!line.contains('\n'), "a control line must be single-line");
        match serde_json::from_str::<NodeMsg>(&line).expect("deserialize") {
            NodeMsg::Hello { proto, node_id, downstreams, .. } => {
                assert_eq!(proto, PROTO_VERSION);
                assert_eq!(node_id, "n1");
                assert_eq!(downstreams.len(), 1);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn hello_tolerates_absent_downstreams() {
        let line = r#"{"t":"hello","proto":1,"node_id":"n1","label":"a"}"#;
        match serde_json::from_str::<NodeMsg>(line).expect("deserialize") {
            NodeMsg::Hello { downstreams, .. } => assert!(downstreams.is_empty()),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn open_message_roundtrips() {
        let msg = RelayMsg::Open {
            stream_id: "s-123".into(),
            target: "d1".into(),
        };
        let line = serde_json::to_string(&msg).expect("serialize");
        assert!(line.contains("\"t\":\"open\""));
        match serde_json::from_str::<RelayMsg>(&line).expect("deserialize") {
            RelayMsg::Open { stream_id, target } => {
                assert_eq!(stream_id, "s-123");
                assert_eq!(target, "d1");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn error_codes_and_statuses_are_frozen() {
        assert_eq!(RelayError::Unauthorized.status(), 401);
        assert_eq!(RelayError::UnknownNode.code(), "unknown_node");
        assert_eq!(RelayError::NodeOffline.status(), 502);
        assert_eq!(RelayError::DownstreamUnreachable.code(), "downstream_unreachable");
    }
}
