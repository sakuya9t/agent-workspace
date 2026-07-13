//! The node-side relay agent.
//!
//! A daemon (or, in tests, a stand-in) runs this to make itself reachable
//! through the relay from behind NAT. It holds one outbound **control WSS** to
//! `/register`, announces itself with [`NodeMsg::Hello`], and pings for
//! liveness. When the relay asks it to serve a client (via [`RelayMsg::Open`]),
//! it dials a fresh outbound **data WSS** to `/data?stream_id=…` and splices
//! that to the resolved local address — its own listener, or an advertised
//! downstream. Every connection the agent makes is outbound, so it works from
//! behind NAT.
//!
//! This module is the reusable half the daemon embeds in R2; the relay binary
//! never runs it.

use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Context, Result};
use futures::{SinkExt, StreamExt};
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::protocol::{
    DownstreamInfo, NodeMsg, RelayMsg, BACKOFF_MAX, BACKOFF_MIN, BACKOFF_STABLE_RESET, DATA_PATH,
    DATA_STREAM_QUERY, PING_INTERVAL, PROTO_VERSION, REGISTER_PATH, RELAY_KEY_QUERY,
};
use crate::transport::WsByteStream;

/// A downstream resolved by the gateway's probe loop (R4): a static address
/// (from config) annotated with the identity and reachability learned by
/// probing the downstream daemon's `/health`. Only downstreams whose `/health`
/// has answered at least once — so their `node_id` is known — ever appear here,
/// because the relay can route only to a known `node_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDownstream {
    pub node_id: String,
    pub label: String,
    pub addr: SocketAddr,
    pub reachable: bool,
}

/// Everything the agent needs to register and serve.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Relay base URL with scheme + authority, no trailing slash and no path,
    /// e.g. `ws://127.0.0.1:4700` or `wss://relay.example.com`. Keys and ids are
    /// appended as query params, so `relay_key` must be URL-safe.
    pub relay_url: String,
    pub relay_key: String,
    pub node_id: String,
    pub label: String,
    /// Where traffic addressed to this node itself is spliced (the daemon's
    /// tunnel listener, or a test server).
    pub local_target: SocketAddr,
    /// Live view of the downstreams this node bridges (R4); empty for a leaf.
    /// The gateway's probe loop publishes updates here; the agent re-advertises
    /// the set over the control stream whenever it changes and resolves `Open`
    /// targets against it. For a leaf, this is a receiver whose value stays the
    /// empty vec (its sender may be dropped — the agent tolerates that).
    pub downstreams: watch::Receiver<Vec<ResolvedDownstream>>,
}

impl AgentConfig {
    /// The downstream set to advertise, taken from the current probe view.
    fn advertised(&self) -> Vec<DownstreamInfo> {
        self.downstreams
            .borrow()
            .iter()
            .map(|d| DownstreamInfo {
                node_id: d.node_id.clone(),
                label: d.label.clone(),
                reachable: d.reachable,
            })
            .collect()
    }

    /// Map an `Open` target to the local address to dial: this node's own tunnel
    /// listener, or one of its advertised downstreams.
    fn resolve(&self, target: &str) -> Option<SocketAddr> {
        if target == self.node_id {
            return Some(self.local_target);
        }
        self.downstreams
            .borrow()
            .iter()
            .find(|d| d.node_id == target)
            .map(|d| d.addr)
    }
}

/// Run the agent forever, reconnecting with jittered exponential backoff. The
/// caller runs this as a task and aborts it to stop (used by the daemon and by
/// tests).
pub async fn run(cfg: AgentConfig) {
    let mut backoff = BACKOFF_MIN;
    loop {
        let started = Instant::now();
        match run_once(&cfg).await {
            Ok(()) => tracing::info!(node = %cfg.node_id, "relay control stream closed; reconnecting"),
            Err(e) => tracing::warn!(node = %cfg.node_id, "relay agent connection error: {e:#}"),
        }
        if started.elapsed() >= BACKOFF_STABLE_RESET {
            backoff = BACKOFF_MIN;
        }
        tokio::time::sleep(jitter(backoff)).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// One connection lifetime: register, then pump control until the socket drops.
async fn run_once(cfg: &AgentConfig) -> Result<()> {
    let url = format!(
        "{}{}?{}={}",
        cfg.relay_url, REGISTER_PATH, RELAY_KEY_QUERY, cfg.relay_key
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("dialing relay control stream at {url}"))?;
    let (mut sink, mut stream) = ws.split();
    tracing::info!(node = %cfg.node_id, "registered control stream with relay");

    // A single writer task owns the sink so pings, pongs, and hello never race.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(m) = out_rx.recv().await {
            if sink.send(m).await.is_err() {
                break;
            }
        }
    });

    let hello = NodeMsg::Hello {
        proto: PROTO_VERSION,
        node_id: cfg.node_id.clone(),
        label: cfg.label.clone(),
        downstreams: cfg.advertised(),
    };
    out_tx.send(Message::Text(serde_json::to_string(&hello)?))?;

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut seq: u64 = 0;

    // A private clone of the downstreams view to await changes on (the shared
    // `cfg.downstreams` is read via `advertised()`/`resolve()`). Starts "seen",
    // so it fires only on updates *after* the hello above — never a duplicate.
    let mut ds_rx = cfg.downstreams.clone();
    let mut watch_live = true;

    let result = loop {
        tokio::select! {
            _ = ping.tick() => {
                seq += 1;
                if out_tx.send(Message::Text(serde_json::to_string(&NodeMsg::Ping { seq })?)).is_err() {
                    break Ok(());
                }
            }
            changed = ds_rx.changed(), if watch_live => match changed {
                // A probe result changed: re-advertise the whole set.
                Ok(()) => {
                    let msg = NodeMsg::Downstreams { downstreams: cfg.advertised() };
                    if out_tx.send(Message::Text(serde_json::to_string(&msg)?)).is_err() {
                        break Ok(());
                    }
                }
                // All senders dropped (a leaf, or the gateway shutting down):
                // stop polling so this branch cannot busy-loop on the error.
                Err(_) => watch_live = false,
            },
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    match serde_json::from_str::<RelayMsg>(t.as_str()) {
                        Ok(RelayMsg::Open { stream_id, target }) => {
                            spawn_data_stream(cfg.clone(), stream_id, target);
                        }
                        Ok(RelayMsg::Pong { .. }) => {}
                        Ok(RelayMsg::Error { code, message }) => {
                            break Err(anyhow::anyhow!("relay refused registration: {code} ({message})"));
                        }
                        Err(e) => tracing::warn!("undecodable relay control message: {e}"),
                    }
                }
                Some(Ok(Message::Ping(p))) => { let _ = out_tx.send(Message::Pong(p)); }
                Some(Ok(Message::Close(_))) | None => break Ok(()),
                Some(Ok(_)) => {} // binary/pong on the control stream: ignore
                Some(Err(e)) => break Err(anyhow::Error::new(e).context("relay control stream read")),
            }
        }
    };

    drop(out_tx);
    writer.abort();
    result
}

/// Dial a data WSS for one client connection and splice it to the local target.
/// Detached: a failed or slow stream never blocks the control loop or siblings.
fn spawn_data_stream(cfg: AgentConfig, stream_id: String, target: String) {
    tokio::spawn(async move {
        if let Err(e) = serve_data_stream(&cfg, &stream_id, &target).await {
            tracing::warn!(%target, %stream_id, "data stream failed: {e:#}");
        }
    });
}

async fn serve_data_stream(cfg: &AgentConfig, stream_id: &str, target: &str) -> Result<()> {
    let Some(addr) = cfg.resolve(target) else {
        bail!("no local target for node_id `{target}`");
    };
    let url = format!(
        "{}{}?{}={}&{}={}",
        cfg.relay_url, DATA_PATH, RELAY_KEY_QUERY, cfg.relay_key, DATA_STREAM_QUERY, stream_id
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .context("dialing relay data stream")?;
    let mut ws_bytes = WsByteStream::new(ws);
    let mut tcp = TcpStream::connect(addr)
        .await
        .with_context(|| format!("dialing local target {addr}"))?;
    copy_bidirectional(&mut ws_bytes, &mut tcp)
        .await
        .context("splicing data stream")?;
    Ok(())
}

/// ±[`crate::protocol::BACKOFF_JITTER`] jitter, sourced from the wall clock's
/// sub-second nanos (no `rand` dependency).
fn jitter(base: Duration) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let frac = (nanos as f64 / 1_000_000_000.0) * 2.0 - 1.0; // -1.0 ..= 1.0
    let factor = 1.0 + crate::protocol::BACKOFF_JITTER * frac;
    base.mul_f64(factor.clamp(0.1, 2.0))
}
