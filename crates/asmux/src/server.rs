//! The UDS server: accept loop, per-connection framing, and RPC/event dispatch.
//!
//! Each connection gets a writer task draining two channels — a **control**
//! channel (responses, events, errors, heartbeats) the writer biases toward, and
//! a **data** channel (`SessionOutput`) — so a backed-up data stream can never
//! starve an eviction event of headroom. Heartbeats come from a dedicated OS
//! thread so a busy async runtime can't delay them (contract → Liveness).
//!
//! Streaming, single-attacher takeover, per-session backpressure eviction, and
//! the input/data plane are all handled here; the ring and PTY live in
//! [`crate::session`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use planus::ReadAsRoot;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Notify};

use crate::frame::{self, code, ord, FrameError, Incoming};
use crate::registry::{launch_fingerprint, CreateError, CreateOutcome, PurgeOutcome, Registry};
use crate::ring::ReadOutcome;
use crate::session::{Attacher, InputOutcome, Session, SpawnSpec, Status};
use crate::wire;
use crate::{
    HEARTBEAT_INTERVAL_MS, PROTOCOL_V1, RING_DEFAULT_BYTES, RING_MAX_BYTES, RING_MIN_BYTES,
};

const CTRL_CHANNEL_CAP: usize = 512;
const DATA_CHANNEL_CAP: usize = 512;
/// Upper bound on a single `readBuffer` response, kept below the frame cap.
const READBUF_MAX: usize = 8 * 1024 * 1024;

/// Shared server state.
pub struct ServerCtx {
    pub registry: Arc<Registry>,
    pub server_pid: i32,
    /// Populated in M4 (soft-reboot / hash drift). Empty for now; `instance_id`
    /// is what proves holder identity.
    pub binary_sha256: String,
    next_conn_id: AtomicU64,
}

impl ServerCtx {
    pub fn new(registry: Arc<Registry>, server_pid: i32, binary_sha256: String) -> Arc<Self> {
        Arc::new(ServerCtx {
            registry,
            server_pid,
            binary_sha256,
            next_conn_id: AtomicU64::new(1),
        })
    }
}

/// One connection's outbound plumbing and its owned streams.
struct Conn {
    id: u64,
    ctx: Arc<ServerCtx>,
    ctrl_tx: mpsc::Sender<Vec<u8>>,
    data_tx: mpsc::Sender<Vec<u8>>,
    hello_done: AtomicBool,
    /// session_id -> cancel handle for this connection's streamer of it.
    streams: Mutex<HashMap<String, Arc<Notify>>>,
}

/// Accept connections forever, one task per connection.
pub async fn serve(listener: UnixListener, ctx: Arc<ServerCtx>) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let ctx = ctx.clone();
                let id = ctx.next_conn_id.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    handle_conn(stream, ctx, id).await;
                });
            }
            Err(e) => {
                tracing::warn!("accept failed: {e}");
            }
        }
    }
}

async fn handle_conn(stream: UnixStream, ctx: Arc<ServerCtx>, id: u64) {
    let (mut rd, wr) = stream.into_split();
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<Vec<u8>>(CTRL_CHANNEL_CAP);
    let (data_tx, data_rx) = mpsc::channel::<Vec<u8>>(DATA_CHANNEL_CAP);

    tokio::spawn(writer_task(wr, ctrl_rx, data_rx));
    spawn_heartbeat(ctrl_tx.clone());

    let conn = Arc::new(Conn {
        id,
        ctx: ctx.clone(),
        ctrl_tx,
        data_tx,
        hello_done: AtomicBool::new(false),
        streams: Mutex::new(HashMap::new()),
    });

    read_loop(&mut rd, &conn).await;

    // Teardown: cancel every streamer this connection owns and detach.
    let owned: Vec<(String, Arc<Notify>)> = conn.streams.lock().drain().collect();
    for (sid, cancel) in owned {
        cancel.notify_one();
        if let Some(s) = ctx.registry.get(&sid) {
            s.detach(id);
        }
    }
    tracing::debug!(conn = id, "connection closed");
}

async fn read_loop(rd: &mut OwnedReadHalf, conn: &Arc<Conn>) {
    loop {
        match frame::read_frame(rd).await {
            Ok(Incoming::Eof) => return,
            Ok(Incoming::Frame { ordinal, body }) => {
                if !conn.hello_done.load(Ordering::Acquire) && ordinal != ord::HELLO_REQUEST {
                    send_error(
                        &conn.ctrl_tx,
                        0,
                        code::PROTOCOL_MISMATCH,
                        None,
                        None,
                        "hello must be the first frame",
                    )
                    .await;
                    return;
                }
                if !dispatch(conn, ordinal, &body).await {
                    return;
                }
            }
            Err(FrameError::TooLarge) => {
                send_error(
                    &conn.ctrl_tx,
                    0,
                    code::FRAME_TOO_LARGE,
                    None,
                    None,
                    "frame exceeds 16 MiB",
                )
                .await;
                return;
            }
            Err(FrameError::Malformed) => {
                send_error(
                    &conn.ctrl_tx,
                    0,
                    code::PROTOCOL_MISMATCH,
                    None,
                    None,
                    "malformed frame",
                )
                .await;
                return;
            }
            Err(FrameError::Io(_)) => return,
        }
    }
}

/// Dispatch one frame. Returns `false` to close the connection.
async fn dispatch(conn: &Arc<Conn>, ordinal: u16, body: &[u8]) -> bool {
    match ordinal {
        ord::HELLO_REQUEST => handle_hello(conn, body).await,
        ord::CREATE_REQUEST => {
            handle_create(conn, body).await;
            true
        }
        ord::KILL_REQUEST => {
            handle_kill(conn, body).await;
            true
        }
        ord::PURGE_REQUEST => {
            handle_purge(conn, body).await;
            true
        }
        ord::LIST_REQUEST => {
            handle_list(conn, body).await;
            true
        }
        ord::UPDATE_METADATA_REQUEST => {
            handle_update_metadata(conn, body).await;
            true
        }
        ord::RESIZE_REQUEST => {
            handle_resize(conn, body).await;
            true
        }
        ord::READ_BUFFER_REQUEST => {
            handle_read_buffer(conn, body).await;
            true
        }
        ord::ATTACH_REQUEST => {
            handle_attach(conn, body).await;
            true
        }
        ord::DETACH_REQUEST => {
            handle_detach(conn, body).await;
            true
        }
        ord::SESSION_INPUT => {
            handle_input(conn, body).await;
            true
        }
        ord::HEARTBEAT => true, // inbound heartbeat: liveness only (M4 watchdog)
        _ => {
            // Reserved / unknown ordinal: report but keep the connection.
            send_error(
                &conn.ctrl_tx,
                0,
                code::INVALID_ARGUMENT,
                None,
                None,
                "unknown ordinal",
            )
            .await;
            true
        }
    }
}

async fn handle_hello(conn: &Arc<Conn>, body: &[u8]) -> bool {
    let r = match wire::HelloRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => {
            send_error(&conn.ctrl_tx, 0, code::PROTOCOL_MISMATCH, None, None, "bad hello").await;
            return false;
        }
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let pmin = r.protocol_min().unwrap_or(0);
    let pmax = r.protocol_max().unwrap_or(0);
    if PROTOCOL_V1 < pmin || PROTOCOL_V1 > pmax {
        send_error(
            &conn.ctrl_tx,
            rpc_id,
            code::PROTOCOL_MISMATCH,
            None,
            None,
            "no overlapping protocol version",
        )
        .await;
        return false;
    }
    conn.hello_done.store(true, Ordering::Release);
    let resp = wire::HelloResponse {
        rpc_id,
        server_pid: conn.ctx.server_pid,
        binary_sha256: Some(conn.ctx.binary_sha256.clone()),
        protocol: PROTOCOL_V1,
        session_count: conn.ctx.registry.session_count(),
        started_at_unix_ms: conn.ctx.registry.started_at_unix_ms,
        instance_id: Some(conn.ctx.registry.instance_id.clone()),
    };
    send(&conn.ctrl_tx, frame::encode(ord::HELLO_RESPONSE, &resp)).await;
    true
}

async fn handle_create(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::CreateRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);

    let command = r.command().ok().flatten().unwrap_or("");
    if command.is_empty() {
        send_error(&conn.ctrl_tx, rpc_id, code::INVALID_ARGUMENT, None, None, "empty command").await;
        return;
    }
    let args = read_string_vec(r.args().ok().flatten());
    let cwd = r.cwd().ok().flatten().unwrap_or("").to_string();
    let env = read_kv_pairs(r.env().ok().flatten());
    let cols = r.cols().unwrap_or(80);
    let rows = r.rows().unwrap_or(24);
    if cols == 0 || rows == 0 {
        send_error(&conn.ctrl_tx, rpc_id, code::INVALID_ARGUMENT, None, None, "zero geometry").await;
        return;
    }
    let cap_req = r.ring_capacity().unwrap_or(0);
    let cap = if cap_req == 0 { RING_DEFAULT_BYTES } else { cap_req };
    if !(RING_MIN_BYTES..=RING_MAX_BYTES).contains(&cap) {
        send_error(
            &conn.ctrl_tx,
            rpc_id,
            code::CAPACITY_OUT_OF_RANGE,
            None,
            None,
            "ring capacity out of [16 KiB, 32 MiB]",
        )
        .await;
        return;
    }
    let metadata = read_kv_pairs(r.metadata().ok().flatten());
    let session_id = r
        .session_id()
        .ok()
        .flatten()
        .map(str::to_string)
        .unwrap_or_else(new_uuid);
    let fingerprint = launch_fingerprint(command, &args, &cwd, &env);

    let spec = SpawnSpec {
        session_id,
        command: command.to_string(),
        args,
        cwd,
        env,
        cols,
        rows,
        ring_capacity: cap as usize,
        metadata,
        fingerprint,
        created_at_unix_ms: now_ms(),
    };

    match conn.ctx.registry.create(spec) {
        Ok(CreateOutcome::Created(s)) | Ok(CreateOutcome::Idempotent(s)) => {
            let resp = wire::CreateResponse {
                rpc_id,
                session: Some(Box::new(s.record())),
            };
            send(&conn.ctrl_tx, frame::encode(ord::CREATE_RESPONSE, &resp)).await;
        }
        Err(CreateError::Exists) => {
            send_error(
                &conn.ctrl_tx,
                rpc_id,
                code::SESSION_EXISTS,
                None,
                None,
                "session id exists with a different launch fingerprint",
            )
            .await;
        }
        Err(CreateError::MemoryLimit) => {
            send_error(
                &conn.ctrl_tx,
                rpc_id,
                code::MEMORY_LIMIT,
                None,
                None,
                "total ring-memory cap reached",
            )
            .await;
        }
        Err(CreateError::Spawn(msg)) => {
            send_error(&conn.ctrl_tx, rpc_id, code::SPAWN_FAILED, None, None, &msg).await;
        }
    }
}

async fn handle_kill(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::KillRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("");
    let signal = r.signal().unwrap_or(0);
    match conn.ctx.registry.get(sid) {
        Some(s) => {
            s.kill(signal); // idempotent on a dead session
            let resp = wire::KillResponse { rpc_id };
            send(&conn.ctrl_tx, frame::encode(ord::KILL_RESPONSE, &resp)).await;
        }
        None => {
            send_error(&conn.ctrl_tx, rpc_id, code::UNKNOWN_SESSION, Some(sid), None, "no such session").await;
        }
    }
}

async fn handle_purge(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::PurgeRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("");
    match conn.ctx.registry.purge(sid) {
        PurgeOutcome::Purged => {
            let resp = wire::PurgeResponse { rpc_id };
            send(&conn.ctrl_tx, frame::encode(ord::PURGE_RESPONSE, &resp)).await;
        }
        PurgeOutcome::Unknown => {
            send_error(&conn.ctrl_tx, rpc_id, code::UNKNOWN_SESSION, Some(sid), None, "no such session").await;
        }
        PurgeOutcome::Alive => {
            send_error(&conn.ctrl_tx, rpc_id, code::SESSION_ALIVE, Some(sid), None, "kill before purge").await;
        }
    }
}

async fn handle_list(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::ListRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sessions: Vec<wire::SessionRecord> =
        conn.ctx.registry.list().iter().map(|s| s.record()).collect();
    let resp = wire::ListResponse {
        rpc_id,
        sessions: Some(sessions),
    };
    send(&conn.ctrl_tx, frame::encode(ord::LIST_RESPONSE, &resp)).await;
}

async fn handle_update_metadata(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::UpdateMetadataRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("");
    let patch = read_kv_patch(r.patch().ok().flatten());
    match conn.ctx.registry.get(sid) {
        Some(s) => {
            s.patch_metadata(&patch);
            let resp = wire::UpdateMetadataResponse {
                rpc_id,
                session: Some(Box::new(s.record())),
            };
            send(&conn.ctrl_tx, frame::encode(ord::UPDATE_METADATA_RESPONSE, &resp)).await;
        }
        None => {
            send_error(&conn.ctrl_tx, rpc_id, code::UNKNOWN_SESSION, Some(sid), None, "no such session").await;
        }
    }
}

async fn handle_resize(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::ResizeRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("");
    let cols = r.cols().unwrap_or(0);
    let rows = r.rows().unwrap_or(0);
    match conn.ctx.registry.get(sid) {
        Some(s) => match s.resize(cols, rows) {
            InputOutcome::NotAlive => {
                send_error(&conn.ctrl_tx, rpc_id, code::SESSION_NOT_ALIVE, Some(sid), None, "session not alive").await;
            }
            _ => {
                let resp = wire::ResizeResponse { rpc_id };
                send(&conn.ctrl_tx, frame::encode(ord::RESIZE_RESPONSE, &resp)).await;
            }
        },
        None => {
            send_error(&conn.ctrl_tx, rpc_id, code::UNKNOWN_SESSION, Some(sid), None, "no such session").await;
        }
    }
}

async fn handle_read_buffer(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::ReadBufferRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("");
    let from = r.from_cursor().unwrap_or(0);
    let max_req = r.max_bytes().unwrap_or(0) as usize;
    let max = if max_req == 0 { READBUF_MAX } else { max_req.min(READBUF_MAX) };
    match conn.ctx.registry.get(sid) {
        Some(s) => {
            let head = s.head();
            match s.read_at(from, max) {
                ReadOutcome::Data { from, data } => {
                    let resp = wire::ReadBufferResponse {
                        rpc_id,
                        from_cursor: from,
                        head_cursor: head,
                        data: Some(data),
                    };
                    send(&conn.ctrl_tx, frame::encode(ord::READ_BUFFER_RESPONSE, &resp)).await;
                }
                ReadOutcome::Gap { earliest } => {
                    send_error(&conn.ctrl_tx, rpc_id, code::BUFFER_GAP, Some(sid), Some(earliest), "cursor older than tail").await;
                }
                ReadOutcome::Invalid => {
                    send_error(&conn.ctrl_tx, rpc_id, code::INVALID_ARGUMENT, Some(sid), None, "cursor past head").await;
                }
            }
        }
        None => {
            send_error(&conn.ctrl_tx, rpc_id, code::UNKNOWN_SESSION, Some(sid), None, "no such session").await;
        }
    }
}

async fn handle_attach(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::AttachRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("").to_string();
    let mode = r.mode().unwrap_or(wire::AttachMode::LiveOnly);
    let from_cursor = r.from_cursor().unwrap_or(0);

    let session = match conn.ctx.registry.get(&sid) {
        Some(s) => s,
        None => {
            send_error(&conn.ctrl_tx, rpc_id, code::UNKNOWN_SESSION, Some(&sid), None, "no such session").await;
            return;
        }
    };

    let head = session.head();
    let tail = session.tail();
    let start = match mode {
        wire::AttachMode::LiveOnly => head,
        wire::AttachMode::FromEarliest => tail,
        wire::AttachMode::FromCursor => {
            if from_cursor > head {
                send_error(&conn.ctrl_tx, rpc_id, code::INVALID_ARGUMENT, Some(&sid), None, "cursor past head").await;
                return;
            }
            if from_cursor < tail {
                send_error(&conn.ctrl_tx, rpc_id, code::BUFFER_GAP, Some(&sid), Some(tail), "cursor older than tail").await;
                return;
            }
            from_cursor
        }
    };

    // Install this connection as the sole attacher (takeover).
    let cancel = Arc::new(Notify::new());
    let attacher = Attacher {
        conn_id: conn.id,
        cancel: cancel.clone(),
        ctrl_tx: conn.ctrl_tx.clone(),
    };
    if let Some(old) = session.attach(attacher) {
        if old.conn_id != conn.id {
            // Cross-connection takeover: notify the evicted connection.
            let ev = wire::SessionDetached {
                session_id: Some(sid.clone()),
                reason: wire::DetachReason::Superseded,
                last_cursor: head,
            };
            let _ = old.ctrl_tx.send(frame::encode(ord::SESSION_DETACHED, &ev)).await;
        }
        old.cancel.notify_one();
    }
    if let Some(prev) = conn.streams.lock().insert(sid.clone(), cancel.clone()) {
        prev.notify_one(); // supersede a prior stream of the same session on this conn
    }

    let live_attach = session.is_alive();
    let resp = wire::AttachResponse {
        rpc_id,
        head_cursor: head,
    };
    send(&conn.ctrl_tx, frame::encode(ord::ATTACH_RESPONSE, &resp)).await;

    tokio::spawn(stream_session(
        session,
        conn.clone(),
        sid,
        start,
        cancel,
        live_attach,
    ));
}

async fn handle_detach(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::DetachRequestRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let rpc_id = r.rpc_id().unwrap_or(0);
    let sid = r.session_id().ok().flatten().unwrap_or("");
    let session = conn.ctx.registry.get(sid);
    let attached = session.as_ref().map(|s| s.is_attached_by(conn.id)).unwrap_or(false);
    if !attached {
        send_error(&conn.ctrl_tx, rpc_id, code::NOT_ATTACHED, Some(sid), None, "not attached").await;
        return;
    }
    if let Some(cancel) = conn.streams.lock().remove(sid) {
        cancel.notify_one();
    }
    if let Some(s) = session {
        s.detach(conn.id);
    }
    let resp = wire::DetachResponse { rpc_id };
    send(&conn.ctrl_tx, frame::encode(ord::DETACH_RESPONSE, &resp)).await;
}

async fn handle_input(conn: &Arc<Conn>, body: &[u8]) {
    let r = match wire::SessionInputRef::read_as_root(body) {
        Ok(r) => r,
        Err(_) => return,
    };
    let sid = r.session_id().ok().flatten().unwrap_or("");
    let data = r.data().ok().flatten();
    let session = match conn.ctx.registry.get(sid) {
        Some(s) => s,
        None => {
            send_error(&conn.ctrl_tx, 0, code::UNKNOWN_SESSION, Some(sid), None, "no such session").await;
            return;
        }
    };
    if !session.is_attached_by(conn.id) {
        send_error(&conn.ctrl_tx, 0, code::NOT_ATTACHED, Some(sid), None, "not attached").await;
        return;
    }
    let Some(data) = data else { return };
    match session.send_input(data) {
        InputOutcome::Queued => {}
        InputOutcome::Overflow => {
            send_error(&conn.ctrl_tx, 0, code::INPUT_OVERFLOW, Some(sid), None, "input queue full").await;
        }
        InputOutcome::NotAlive => {
            send_error(&conn.ctrl_tx, 0, code::SESSION_NOT_ALIVE, Some(sid), None, "session not alive").await;
        }
    }
}

/// Stream `session` to its attacher from `start`, live-following the ring until
/// superseded, evicted (backpressure), the session exits, or the socket dies.
async fn stream_session(
    session: Arc<Session>,
    conn: Arc<Conn>,
    sid: String,
    start: u64,
    cancel: Arc<Notify>,
    live_attach: bool,
) {
    let mut cursor = start;
    loop {
        // Created before the read so a wake that races it is not lost (`Notify`
        // stores a permit).
        let woken = session.notified();
        match session.read_at(cursor, 0) {
            ReadOutcome::Invalid => break,
            ReadOutcome::Gap { earliest: _ } => {
                // The ring wrapped past our cursor: evict just this session.
                let ev = wire::SessionDetached {
                    session_id: Some(sid.clone()),
                    reason: wire::DetachReason::Backpressure,
                    last_cursor: cursor,
                };
                let _ = conn.ctrl_tx.send(frame::encode(ord::SESSION_DETACHED, &ev)).await;
                break;
            }
            ReadOutcome::Data { from, data } => {
                if !data.is_empty() {
                    cursor = from.saturating_add(data.len() as u64);
                    let out = wire::SessionOutput {
                        session_id: Some(sid.clone()),
                        head_cursor: cursor,
                        data: Some(data),
                    };
                    let frame = frame::encode(ord::SESSION_OUTPUT, &out);
                    tokio::select! {
                        biased;
                        _ = cancel.notified() => break,
                        r = conn.data_tx.send(frame) => {
                            if r.is_err() { break; }
                        }
                    }
                    continue;
                }
                // Caught up to head.
                if !session.is_alive() && cursor >= session.head() {
                    if live_attach {
                        let (exit_code, exit_signal) = match session.status() {
                            Status::Exited { code, signal } => (code, signal),
                            Status::Running => (0, 0),
                        };
                        let ev = wire::SessionExited {
                            session_id: Some(sid.clone()),
                            exit_code,
                            exit_signal,
                            head_cursor: session.head(),
                        };
                        let _ = conn.ctrl_tx.send(frame::encode(ord::SESSION_EXITED, &ev)).await;
                    }
                    break;
                }
                tokio::select! {
                    biased;
                    _ = cancel.notified() => break,
                    _ = woken => {}
                }
            }
        }
    }

    // Drop this stream's registration if it is still ours, then detach.
    {
        let mut streams = conn.streams.lock();
        let ours = streams.get(&sid).map(|c| Arc::ptr_eq(c, &cancel)).unwrap_or(false);
        if ours {
            streams.remove(&sid);
        }
    }
    session.detach(conn.id);
}

// ---- outbound helpers -------------------------------------------------------

async fn send(tx: &mpsc::Sender<Vec<u8>>, frame: Vec<u8>) {
    let _ = tx.send(frame).await;
}

async fn send_error(
    tx: &mpsc::Sender<Vec<u8>>,
    rpc_id: u64,
    code: u32,
    session_id: Option<&str>,
    earliest: Option<u64>,
    message: &str,
) {
    let err = wire::Error {
        rpc_id,
        code,
        message: Some(message.to_string()),
        session_id: session_id.map(str::to_string),
        earliest_cursor: earliest.unwrap_or(0),
    };
    let _ = tx.send(frame::encode(ord::ERROR, &err)).await;
}

/// Per-connection writer: bias control frames over data so an eviction event
/// always has headroom even when the data channel is backed up.
async fn writer_task(
    mut wr: OwnedWriteHalf,
    mut ctrl_rx: mpsc::Receiver<Vec<u8>>,
    mut data_rx: mpsc::Receiver<Vec<u8>>,
) {
    loop {
        tokio::select! {
            biased;
            Some(f) = ctrl_rx.recv() => {
                if wr.write_all(&f).await.is_err() { break; }
            }
            Some(f) = data_rx.recv() => {
                if wr.write_all(&f).await.is_err() { break; }
            }
            else => break,
        }
    }
    let _ = wr.shutdown().await;
}

/// Dedicated OS thread emitting a heartbeat every second (contract → Liveness).
fn spawn_heartbeat(ctrl_tx: mpsc::Sender<Vec<u8>>) {
    let _ = std::thread::Builder::new()
        .name("asmux-hb".to_string())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(HEARTBEAT_INTERVAL_MS));
            let hb = wire::Heartbeat { unix_ms: now_ms() };
            if ctrl_tx.blocking_send(frame::encode(ord::HEARTBEAT, &hb)).is_err() {
                break;
            }
        });
}

// ---- decode helpers ---------------------------------------------------------

fn read_string_vec(v: Option<planus::Vector<'_, planus::Result<&str>>>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(v) = v {
        for s in v.iter().flatten() {
            out.push(s.to_string());
        }
    }
    out
}

fn read_kv_pairs(v: Option<planus::Vector<'_, planus::Result<wire::KvRef<'_>>>>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(v) = v {
        for kv in v.iter().flatten() {
            let k = kv.key().ok().flatten().unwrap_or("").to_string();
            let val = kv.value().ok().flatten().unwrap_or("").to_string();
            out.push((k, val));
        }
    }
    out
}

/// For `updateMetadata`: `Some(v)` sets (incl. ""), `None` deletes.
fn read_kv_patch(
    v: Option<planus::Vector<'_, planus::Result<wire::KvRef<'_>>>>,
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    if let Some(v) = v {
        for kv in v.iter().flatten() {
            let k = kv.key().ok().flatten().unwrap_or("").to_string();
            if k.is_empty() {
                continue;
            }
            let val = kv.value().ok().flatten().map(str::to_string);
            out.push((k, val));
        }
    }
    out
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
