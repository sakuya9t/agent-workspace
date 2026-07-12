//! Async, **reconnecting** client for the asmux holder (the daemon is the
//! *client*; asmux is the *server*). One `UnixStream` multiplexes every session.
//!
//! A single **supervisor task** owns the socket lifecycle: dial → `hello` →
//! re-attach every routed session → drain the command channel to the writer,
//! with exponential backoff on a drop and a 10 s idle watchdog (asmux sends 1 Hz
//! heartbeats; ten seconds of silence means the socket is wedged — tear it down
//! and reconnect). The public handle (`cmd_tx`, `routes`, `pending`) is stable
//! across reconnects, so `SidecarSession` and the fire-and-forget senders never
//! observe the churn; drain tasks keep their per-session route registered and
//! resume from `FromCursor(last_cursor)` after each reconnect.
//!
//! `read_frame` is not cancellation-safe (sequential `read_exact`s), so a fresh
//! **reader task** owns each connection's read half and is never `select!`ed on
//! mid-frame; the supervisor learns the socket died by the reader task ending.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot, Notify};
use tokio::task::JoinHandle;

use asmux::frame::{self, code, ord, Incoming};
use asmux::wire;
use planus::ReadAsRoot;

use crate::util::now_millis;

use super::BackendSpawnSpec;

/// Outbound heartbeat cadence and the idle-teardown threshold (see
/// `asmux-protocol.md` → Liveness). The watchdog fires at `IDLE_TEARDOWN_MS` of
/// no inbound frames of any kind.
const HEARTBEAT_MS: u64 = 5_000;
const WATCHDOG_TICK_MS: u64 = 2_000;
const IDLE_TEARDOWN_MS: i64 = 10_000;
/// Reconnect backoff bounds.
const BACKOFF_MIN_MS: u64 = 100;
const BACKOFF_MAX_MS: u64 = 5_000;

/// An event routed to one session's drain task.
#[derive(Debug)]
pub enum StreamEvent {
    /// Live/replayed output. `cursor` is the asmux ring cursor *after* this chunk.
    Output { data: Vec<u8>, cursor: u64 },
    Exited { code: i32, signal: i32 },
    /// Server-initiated eviction (`DetachReason` value); backpressure/superseded.
    Detached { reason: i8 },
}

/// A session as the holder reports it (from `create`/`list`).
#[derive(Debug, Clone)]
pub struct HolderSessionInfo {
    pub id: String,
    pub alive: bool,
    pub exit_code: i32,
    pub exit_signal: i32,
    /// Holder ring head at report time. Reserved for the exact cold-stitch adopt
    /// path; the current adopt seeds from cold history.
    #[allow(dead_code)]
    pub head_cursor: u64,
}

/// The result of an `attach` attempt, distinguishing a recoverable ring gap.
pub enum AttachError {
    /// `from_cursor` older than the ring tail; `earliest` is the new tail.
    Gap { earliest: u64 },
    /// Any other asmux error code.
    Code(u32),
    /// Transport/connection failure.
    Conn(anyhow::Error),
}

/// Emitted whenever the client (re)establishes and re-attaches the holder
/// connection. The daemon subscribes to run a `list`-reconcile after each
/// reconnect (catches exits missed while detached).
#[derive(Clone, Debug)]
pub enum ReconnectEvent {
    Connected,
}

/// One session's output route: where to deliver its `StreamEvent`s and the last
/// ring cursor the reader delivered (the re-attach resume point after a drop).
struct Route {
    tx: mpsc::UnboundedSender<StreamEvent>,
    last_cursor: AtomicU64,
}

type Pending = Mutex<HashMap<u64, oneshot::Sender<(u16, Vec<u8>)>>>;
type Routes = Mutex<HashMap<String, Arc<Route>>>;

/// State shared between the public handle, the supervisor, and the reader — all
/// of it survives reconnects.
struct Shared {
    pending: Pending,
    routes: Routes,
    next_rpc_id: AtomicU64,
    /// Wall-clock ms of the last inbound frame (drives the idle watchdog).
    last_frame_ms: AtomicI64,
    reconnect_tx: broadcast::Sender<ReconnectEvent>,
    /// Test/debug hook: notify to force the current connection down.
    force_drop: Notify,
}

impl Shared {
    fn alloc(&self) -> u64 {
        self.next_rpc_id.fetch_add(1, Ordering::Relaxed)
    }
}

enum ClientCmd {
    Rpc {
        rpc_id: u64,
        frame: Vec<u8>,
        reply: oneshot::Sender<(u16, Vec<u8>)>,
    },
    Fire(Vec<u8>),
}

pub struct AsmuxClient {
    cmd_tx: mpsc::UnboundedSender<ClientCmd>,
    shared: Arc<Shared>,
    pub instance_id: String,
    pub server_pid: i32,
}

impl AsmuxClient {
    /// Connect, run the `hello` handshake, and return a ready client whose
    /// supervisor keeps the connection alive across drops. Fails fast if the
    /// holder is unreachable at startup (matching the one-shot connect it
    /// replaces); after the first success, drops are retried forever.
    pub async fn connect(sock: &Path) -> Result<Arc<AsmuxClient>> {
        let (reconnect_tx, _) = broadcast::channel(16);
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            routes: Mutex::new(HashMap::new()),
            next_rpc_id: AtomicU64::new(1),
            last_frame_ms: AtomicI64::new(now_millis()),
            reconnect_tx,
            force_drop: Notify::new(),
        });
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ClientCmd>();

        // First connection inline so an unreachable holder fails startup.
        let (mut wr, reader) = dial(sock, &shared).await?;
        let hello = do_hello(&mut wr, &shared).await?;

        tokio::spawn(supervisor(
            sock.to_path_buf(),
            shared.clone(),
            cmd_rx,
            Some((wr, reader)),
        ));

        Ok(Arc::new(AsmuxClient {
            cmd_tx,
            shared,
            instance_id: hello.instance_id,
            server_pid: hello.server_pid,
        }))
    }

    /// Subscribe to reconnect notifications (a fresh receiver misses the initial
    /// connect, which the caller handles via the separate startup reconcile).
    pub fn reconnect_events(&self) -> broadcast::Receiver<ReconnectEvent> {
        self.shared.reconnect_tx.subscribe()
    }

    /// Force the current connection down, triggering a reconnect. Used by tests
    /// to exercise the supervisor without killing the holder process.
    #[allow(dead_code)]
    pub fn force_drop(&self) {
        self.shared.force_drop.notify_one();
    }

    async fn rpc(&self, rpc_id: u64, frame: Vec<u8>) -> Result<(u16, Vec<u8>)> {
        send_rpc(&self.cmd_tx, rpc_id, frame).await
    }
}

#[async_trait]
pub trait Holder: Send + Sync {
    /// Create a session (idempotent on `session_id`).
    async fn create(&self, spec: &BackendSpawnSpec) -> Result<HolderSessionInfo>;
    /// List all sessions the holder knows about (live + tombstones).
    async fn list(&self) -> Result<Vec<HolderSessionInfo>>;
    /// Attach to a session's output stream; returns the head cursor on success.
    async fn attach(
        &self,
        session_id: &str,
        mode: wire::AttachMode,
        from_cursor: u64,
    ) -> std::result::Result<u64, AttachError>;
    fn send_input(&self, session_id: &str, data: &[u8]);
    fn resize(&self, session_id: &str, cols: u16, rows: u16);
    fn kill(&self, session_id: &str, signal: i32);
    /// Register a per-session output route and return its receiver.
    fn route(&self, session_id: &str) -> mpsc::UnboundedReceiver<StreamEvent>;
    fn unroute(&self, session_id: &str);
    /// Deliver a synthetic `Exited` into a session's route so its drain finalizes
    /// through the normal exit path. Used by reconnect reconciliation when a
    /// `list` reveals a session exited while the daemon was detached.
    fn inject_exit(&self, session_id: &str, code: i32, signal: i32);
}

#[async_trait]
impl Holder for AsmuxClient {
    async fn create(&self, spec: &BackendSpawnSpec) -> Result<HolderSessionInfo> {
        let id = self.shared.alloc();
        let env: Vec<wire::Kv> = spec
            .env
            .iter()
            .map(|(k, v)| wire::Kv {
                key: Some(k.clone()),
                value: Some(v.clone()),
            })
            .collect();
        let req = wire::CreateRequest {
            rpc_id: id,
            command: Some(spec.command.clone()),
            args: Some(spec.args.clone()),
            cwd: Some(spec.cwd.clone()),
            env: Some(env),
            cols: spec.cols,
            rows: spec.rows,
            metadata: None,
            ring_capacity: 0,
            session_id: Some(spec.session_id.clone()),
        };
        let (ordinal, body) = self.rpc(id, frame::encode(ord::CREATE_REQUEST, &req)).await?;
        if ordinal == ord::CREATE_RESPONSE {
            let r = wire::CreateResponseRef::read_as_root(&body)
                .map_err(|e| anyhow!("bad create response: {e}"))?;
            let rec = r
                .session()
                .ok()
                .flatten()
                .ok_or_else(|| anyhow!("create response missing session"))?;
            record_to_info(&rec)
        } else if ordinal == ord::ERROR {
            Err(error_from(&body))
        } else {
            bail!("unexpected reply ordinal {ordinal} to create")
        }
    }

    async fn list(&self) -> Result<Vec<HolderSessionInfo>> {
        let id = self.shared.alloc();
        let req = wire::ListRequest { rpc_id: id };
        let (ordinal, body) = self.rpc(id, frame::encode(ord::LIST_REQUEST, &req)).await?;
        if ordinal == ord::LIST_RESPONSE {
            let r = wire::ListResponseRef::read_as_root(&body)
                .map_err(|e| anyhow!("bad list response: {e}"))?;
            let mut out = Vec::new();
            if let Some(v) = r.sessions().ok().flatten() {
                for rec in v.iter().flatten() {
                    if let Ok(info) = record_to_info(&rec) {
                        out.push(info);
                    }
                }
            }
            Ok(out)
        } else if ordinal == ord::ERROR {
            Err(error_from(&body))
        } else {
            bail!("unexpected reply ordinal {ordinal} to list")
        }
    }

    async fn attach(
        &self,
        session_id: &str,
        mode: wire::AttachMode,
        from_cursor: u64,
    ) -> std::result::Result<u64, AttachError> {
        let id = self.shared.alloc();
        let frame = attach_frame(id, session_id, mode, from_cursor);
        let (ordinal, body) = self.rpc(id, frame).await.map_err(AttachError::Conn)?;
        parse_attach_reply(ordinal, &body)
    }

    fn send_input(&self, session_id: &str, data: &[u8]) {
        let req = wire::SessionInput {
            session_id: Some(session_id.to_string()),
            data: Some(data.to_vec()),
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::SESSION_INPUT, &req)));
    }

    fn resize(&self, session_id: &str, cols: u16, rows: u16) {
        let req = wire::ResizeRequest {
            rpc_id: self.shared.alloc(),
            session_id: Some(session_id.to_string()),
            cols,
            rows,
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::RESIZE_REQUEST, &req)));
    }

    fn kill(&self, session_id: &str, signal: i32) {
        let req = wire::KillRequest {
            rpc_id: self.shared.alloc(),
            session_id: Some(session_id.to_string()),
            signal,
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::KILL_REQUEST, &req)));
    }

    fn route(&self, session_id: &str) -> mpsc::UnboundedReceiver<StreamEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.shared.routes.lock().insert(
            session_id.to_string(),
            Arc::new(Route {
                tx,
                last_cursor: AtomicU64::new(0),
            }),
        );
        rx
    }

    fn unroute(&self, session_id: &str) {
        self.shared.routes.lock().remove(session_id);
    }

    fn inject_exit(&self, session_id: &str, code: i32, signal: i32) {
        if let Some(route) = self.shared.routes.lock().get(session_id) {
            let _ = route.tx.send(StreamEvent::Exited { code, signal });
        }
    }
}

async fn send_rpc(
    cmd_tx: &mpsc::UnboundedSender<ClientCmd>,
    rpc_id: u64,
    frame: Vec<u8>,
) -> Result<(u16, Vec<u8>)> {
    let (tx, rx) = oneshot::channel();
    cmd_tx
        .send(ClientCmd::Rpc {
            rpc_id,
            frame,
            reply: tx,
        })
        .map_err(|_| anyhow!("asmux client is closed"))?;
    match tokio::time::timeout(Duration::from_secs(10), rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => bail!("asmux connection closed before reply"),
        Err(_) => bail!("asmux rpc timed out"),
    }
}

// ---- connection management (supervisor / reader) ----

struct Hello {
    instance_id: String,
    server_pid: i32,
}

/// Dial a fresh socket, spawn its reader, and return the write half + reader
/// handle. Stamps `last_frame_ms` so the watchdog starts from "just connected".
async fn dial(sock: &Path, shared: &Arc<Shared>) -> Result<(OwnedWriteHalf, JoinHandle<()>)> {
    let stream = UnixStream::connect(sock)
        .await
        .with_context(|| format!("connecting to asmux at {}", sock.display()))?;
    shared.last_frame_ms.store(now_millis(), Ordering::Relaxed);
    let (rd, wr) = stream.into_split();
    let reader = tokio::spawn(reader_task(rd, shared.clone()));
    Ok((wr, reader))
}

/// Send `hello` (which MUST be the first frame) directly on the write half and
/// await its reply via the pending map (the reader routes it).
async fn do_hello(wr: &mut OwnedWriteHalf, shared: &Arc<Shared>) -> Result<Hello> {
    let rpc_id = shared.alloc();
    let req = wire::HelloRequest {
        rpc_id,
        client_pid: std::process::id() as i32,
        client_name: Some("asm-daemon".to_string()),
        protocol_min: 1,
        protocol_max: 1,
    };
    let (ordinal, body) =
        rpc_direct(wr, shared, rpc_id, frame::encode(ord::HELLO_REQUEST, &req)).await?;
    if ordinal != ord::HELLO_RESPONSE {
        bail!("asmux hello rejected (ordinal {ordinal})");
    }
    let hr = wire::HelloResponseRef::read_as_root(&body)
        .map_err(|e| anyhow!("bad hello response: {e}"))?;
    Ok(Hello {
        instance_id: hr.instance_id().ok().flatten().unwrap_or("").to_string(),
        server_pid: hr.server_pid().unwrap_or(0),
    })
}

/// Re-attach every routed session after a reconnect, resuming from the last
/// cursor the reader delivered. A ring gap (`from_cursor` wrapped past the tail
/// while detached) falls back to `FromEarliest`; the visible gap marker for that
/// span is rendered daemon-side (Stage B).
async fn reattach_all(wr: &mut OwnedWriteHalf, shared: &Arc<Shared>) -> Result<()> {
    let sessions: Vec<(String, u64)> = shared
        .routes
        .lock()
        .iter()
        .map(|(id, r)| (id.clone(), r.last_cursor.load(Ordering::Relaxed)))
        .collect();
    for (id, cursor) in sessions {
        let rpc_id = shared.alloc();
        let frame = attach_frame(rpc_id, &id, wire::AttachMode::FromCursor, cursor);
        match rpc_direct(wr, shared, rpc_id, frame).await {
            Ok((ordinal, body)) => match parse_attach_reply(ordinal, &body) {
                Ok(_head) => {}
                Err(AttachError::Gap { earliest }) => {
                    tracing::warn!(session = %id, earliest, "reattach gap; resyncing FromEarliest");
                    let rid = shared.alloc();
                    let f = attach_frame(rid, &id, wire::AttachMode::FromEarliest, 0);
                    let _ = rpc_direct(wr, shared, rid, f).await?;
                }
                Err(AttachError::Code(c)) => {
                    tracing::warn!(session = %id, code = c, "reattach failed");
                }
                Err(AttachError::Conn(e)) => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Issue one RPC directly on the write half (used before the writer loop owns
/// it: hello + re-attach). Inserts the pending entry, writes, and awaits.
async fn rpc_direct(
    wr: &mut OwnedWriteHalf,
    shared: &Arc<Shared>,
    rpc_id: u64,
    frame: Vec<u8>,
) -> Result<(u16, Vec<u8>)> {
    let (tx, rx) = oneshot::channel();
    shared.pending.lock().insert(rpc_id, tx);
    wr.write_all(&frame)
        .await
        .context("writing frame to asmux")?;
    match tokio::time::timeout(Duration::from_secs(10), rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => bail!("asmux connection closed before reply"),
        Err(_) => bail!("asmux rpc timed out"),
    }
}

/// Owns the connection lifecycle across the whole client's life: run the current
/// connection until it drops, then reconnect (dial → hello → re-attach) with
/// exponential backoff, forever. Returns only when the client handle is dropped.
async fn supervisor(
    sock: PathBuf,
    shared: Arc<Shared>,
    mut cmd_rx: mpsc::UnboundedReceiver<ClientCmd>,
    initial: Option<(OwnedWriteHalf, JoinHandle<()>)>,
) {
    let mut conn = initial;
    let mut backoff = BACKOFF_MIN_MS;
    loop {
        let (wr, reader) = match conn.take() {
            Some(c) => c,
            None => match reconnect(&sock, &shared).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("asmux reconnect failed: {e:#}; retrying in {backoff}ms");
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX_MS);
                    continue;
                }
            },
        };
        backoff = BACKOFF_MIN_MS;
        // A fresh subscriber (the reconnect consumer) misses the initial connect
        // send (no receivers yet) — that path is covered by the startup reconcile.
        let _ = shared.reconnect_tx.send(ReconnectEvent::Connected);
        match run_writer(wr, &mut cmd_rx, &shared, reader).await {
            WriterEnd::ClientGone => return,
            WriterEnd::Disconnected => {}
        }
    }
}

/// Establish a replacement connection and re-attach live sessions onto it.
async fn reconnect(sock: &Path, shared: &Arc<Shared>) -> Result<(OwnedWriteHalf, JoinHandle<()>)> {
    let (mut wr, reader) = dial(sock, shared).await?;
    // If hello/reattach fails the socket is bad; drop the reader and surface the
    // error so the supervisor backs off and retries.
    if let Err(e) = do_hello(&mut wr, shared).await {
        reader.abort();
        return Err(e);
    }
    if let Err(e) = reattach_all(&mut wr, shared).await {
        reader.abort();
        return Err(e);
    }
    Ok((wr, reader))
}

enum WriterEnd {
    /// The client handle was dropped; stop entirely.
    ClientGone,
    /// The socket died (reader ended, a write failed, the watchdog fired, or a
    /// forced drop); reconnect.
    Disconnected,
}

/// Drive one live connection: drain the command channel to the writer, emit
/// heartbeats, watch for idle teardown, and end when the socket dies.
async fn run_writer(
    mut wr: OwnedWriteHalf,
    cmd_rx: &mut mpsc::UnboundedReceiver<ClientCmd>,
    shared: &Arc<Shared>,
    mut reader: JoinHandle<()>,
) -> WriterEnd {
    let mut heartbeat = tokio::time::interval(Duration::from_millis(HEARTBEAT_MS));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut watchdog = tokio::time::interval(Duration::from_millis(WATCHDOG_TICK_MS));
    watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let outcome = loop {
        tokio::select! {
            biased;
            // The reader task ends only when the socket is gone.
            _ = &mut reader => break WriterEnd::Disconnected,
            _ = shared.force_drop.notified() => break WriterEnd::Disconnected,
            cmd = cmd_rx.recv() => match cmd {
                Some(ClientCmd::Rpc { rpc_id, frame, reply }) => {
                    shared.pending.lock().insert(rpc_id, reply);
                    if wr.write_all(&frame).await.is_err() {
                        break WriterEnd::Disconnected;
                    }
                }
                Some(ClientCmd::Fire(frame)) => {
                    if wr.write_all(&frame).await.is_err() {
                        break WriterEnd::Disconnected;
                    }
                }
                None => break WriterEnd::ClientGone,
            },
            _ = heartbeat.tick() => {
                let hb = wire::Heartbeat { unix_ms: now_millis() };
                if wr.write_all(&frame::encode(ord::HEARTBEAT, &hb)).await.is_err() {
                    break WriterEnd::Disconnected;
                }
            }
            _ = watchdog.tick() => {
                let idle = now_millis() - shared.last_frame_ms.load(Ordering::Relaxed);
                if idle > IDLE_TEARDOWN_MS {
                    tracing::warn!(idle_ms = idle, "asmux idle watchdog fired; reconnecting");
                    break WriterEnd::Disconnected;
                }
            }
        }
    };
    reader.abort();
    outcome
}

async fn reader_task(mut rd: OwnedReadHalf, shared: Arc<Shared>) {
    loop {
        match frame::read_frame(&mut rd).await {
            Ok(Incoming::Eof) => break,
            Ok(Incoming::Frame { ordinal, body }) => {
                shared.last_frame_ms.store(now_millis(), Ordering::Relaxed);
                route_frame(ordinal, &body, &shared);
            }
            Err(_) => break,
        }
    }
    // Connection gone: fail in-flight RPCs fast so their callers don't hang for
    // the full timeout. Routes are kept — the supervisor re-attaches them.
    shared.pending.lock().clear();
}

fn route_frame(ordinal: u16, body: &[u8], shared: &Arc<Shared>) {
    match ordinal {
        ord::SESSION_OUTPUT => {
            if let Ok(r) = wire::SessionOutputRef::read_as_root(body) {
                let sid = r.session_id().ok().flatten().unwrap_or("");
                let cursor = r.head_cursor().unwrap_or(0);
                let data = r.data().ok().flatten().unwrap_or(&[]).to_vec();
                if let Some(route) = shared.routes.lock().get(sid) {
                    route.last_cursor.store(cursor, Ordering::Relaxed);
                    let _ = route.tx.send(StreamEvent::Output { data, cursor });
                }
            }
        }
        ord::SESSION_EXITED => {
            if let Ok(r) = wire::SessionExitedRef::read_as_root(body) {
                let sid = r.session_id().ok().flatten().unwrap_or("");
                let ev = StreamEvent::Exited {
                    code: r.exit_code().unwrap_or(-1),
                    signal: r.exit_signal().unwrap_or(0),
                };
                if let Some(route) = shared.routes.lock().get(sid) {
                    let _ = route.tx.send(ev);
                }
            }
        }
        ord::SESSION_DETACHED => {
            if let Ok(r) = wire::SessionDetachedRef::read_as_root(body) {
                let sid = r.session_id().ok().flatten().unwrap_or("");
                let reason = r.reason().map(|x| x as i8).unwrap_or(-1);
                if let Some(route) = shared.routes.lock().get(sid) {
                    let _ = route.tx.send(StreamEvent::Detached { reason });
                }
            }
        }
        ord::HEARTBEAT => {}
        _ => {
            if let Some(rid) = reply_rpc_id(ordinal, body) {
                if let Some(tx) = shared.pending.lock().remove(&rid) {
                    let _ = tx.send((ordinal, body.to_vec()));
                } else if ordinal == ord::ERROR {
                    // Unsolicited data-plane error (rpc_id=0, e.g. INPUT_OVERFLOW).
                    if let Ok(r) = wire::ErrorRef::read_as_root(body) {
                        tracing::debug!(
                            code = r.code().unwrap_or(0),
                            session = r.session_id().ok().flatten().unwrap_or(""),
                            "asmux unsolicited error"
                        );
                    }
                }
            }
        }
    }
}

fn attach_frame(rpc_id: u64, session_id: &str, mode: wire::AttachMode, from_cursor: u64) -> Vec<u8> {
    let req = wire::AttachRequest {
        rpc_id,
        session_id: Some(session_id.to_string()),
        mode,
        from_cursor,
    };
    frame::encode(ord::ATTACH_REQUEST, &req)
}

fn parse_attach_reply(ordinal: u16, body: &[u8]) -> std::result::Result<u64, AttachError> {
    if ordinal == ord::ATTACH_RESPONSE {
        let r = wire::AttachResponseRef::read_as_root(body)
            .map_err(|e| AttachError::Conn(anyhow!("bad attach response: {e}")))?;
        Ok(r.head_cursor().unwrap_or(0))
    } else if ordinal == ord::ERROR {
        let r = wire::ErrorRef::read_as_root(body)
            .map_err(|e| AttachError::Conn(anyhow!("bad error frame: {e}")))?;
        let c = r.code().unwrap_or(code::INTERNAL);
        if c == code::BUFFER_GAP {
            Err(AttachError::Gap {
                earliest: r.earliest_cursor().unwrap_or(0),
            })
        } else {
            Err(AttachError::Code(c))
        }
    } else {
        Err(AttachError::Conn(anyhow!("unexpected attach reply {ordinal}")))
    }
}

/// Extract the `rpc_id` (field 0) of a reply-carrying frame, else `None`.
fn reply_rpc_id(ordinal: u16, body: &[u8]) -> Option<u64> {
    match ordinal {
        ord::HELLO_RESPONSE => wire::HelloResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::CREATE_RESPONSE => wire::CreateResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::KILL_RESPONSE => wire::KillResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::PURGE_RESPONSE => wire::PurgeResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::LIST_RESPONSE => wire::ListResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::UPDATE_METADATA_RESPONSE => wire::UpdateMetadataResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::RESIZE_RESPONSE => wire::ResizeResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::READ_BUFFER_RESPONSE => wire::ReadBufferResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::ATTACH_RESPONSE => wire::AttachResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::DETACH_RESPONSE => wire::DetachResponseRef::read_as_root(body).ok()?.rpc_id().ok(),
        ord::ERROR => wire::ErrorRef::read_as_root(body).ok()?.rpc_id().ok(),
        _ => None,
    }
}

fn record_to_info(rec: &wire::SessionRecordRef<'_>) -> Result<HolderSessionInfo> {
    Ok(HolderSessionInfo {
        id: rec.id().ok().flatten().unwrap_or("").to_string(),
        alive: rec.alive().unwrap_or(false),
        exit_code: rec.exit_code().unwrap_or(-1),
        exit_signal: rec.exit_signal().unwrap_or(0),
        head_cursor: rec.head_cursor().unwrap_or(0),
    })
}

fn error_from(body: &[u8]) -> anyhow::Error {
    match wire::ErrorRef::read_as_root(body) {
        Ok(r) => {
            let code = r.code().unwrap_or(0);
            let msg = r.message().ok().flatten().unwrap_or("asmux error");
            anyhow!("asmux error {code}: {msg}")
        }
        Err(e) => anyhow!("asmux error (unparseable): {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asmux::registry::Registry;
    use asmux::server::{serve, ServerCtx};
    use asmux::MEMORY_LIMIT_DEFAULT_BYTES;
    use tokio::net::UnixListener;

    /// Await the next non-empty `Output` chunk on a route, or `None` on timeout.
    async fn next_output(
        rx: &mut mpsc::UnboundedReceiver<StreamEvent>,
        dur: Duration,
    ) -> Option<Vec<u8>> {
        tokio::time::timeout(dur, async {
            loop {
                match rx.recv().await {
                    Some(StreamEvent::Output { data, .. }) if !data.is_empty() => return Some(data),
                    Some(_) => continue,
                    None => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    /// A forced connection drop reconnects to the still-alive holder and resumes
    /// the same session's stream (no daemon restart, no lost PTY).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reconnects_and_resumes_stream_after_forced_drop() {
        let sock = std::env::temp_dir().join(format!("asmux-reconn-{}.sock", uuid::Uuid::new_v4()));
        let listener = UnixListener::bind(&sock).unwrap();
        let registry = Arc::new(Registry::new(
            "test-instance".to_string(),
            0,
            MEMORY_LIMIT_DEFAULT_BYTES,
        ));
        let ctx = ServerCtx::new(registry, std::process::id() as i32, String::new());
        tokio::spawn(serve(listener, ctx));

        let client = AsmuxClient::connect(&sock).await.unwrap();
        let spec = BackendSpawnSpec {
            session_id: "reconn".to_string(),
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "i=0; while true; do echo line$i; i=$((i+1)); sleep 0.05; done".to_string(),
            ],
            env: vec![],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            rows: 24,
            cols: 80,
        };
        client.create(&spec).await.unwrap();
        let mut rx = client.route("reconn");
        match client.attach("reconn", wire::AttachMode::FromEarliest, 0).await {
            Ok(_) => {}
            Err(_) => panic!("initial attach failed"),
        }

        // Output flows before the drop.
        assert!(
            next_output(&mut rx, Duration::from_secs(3)).await.is_some(),
            "output before drop"
        );

        // Force the connection down and expect a reconnect notification.
        let mut reconnects = client.reconnect_events();
        client.force_drop();
        tokio::time::timeout(Duration::from_secs(3), reconnects.recv())
            .await
            .expect("reconnect within 3s")
            .expect("reconnect event");

        // Output resumes on the *same* route after the supervisor re-attaches.
        assert!(
            next_output(&mut rx, Duration::from_secs(3)).await.is_some(),
            "output resumes after reconnect"
        );

        client.kill("reconn", 9);
        let _ = std::fs::remove_file(&sock);
    }
}
