//! Async client for the asmux holder (the daemon is the *client*; asmux is the
//! *server*). One `UnixStream` multiplexes every session.
//!
//! Two tasks own the socket: a **reader** that demultiplexes incoming frames
//! (RPC replies by `rpc_id`, per-session `SessionOutput`/`SessionExited`/
//! `SessionDetached` by `session_id`) and a **writer** driven by a command
//! channel. They are split deliberately: `frame::read_frame` is not
//! cancellation-safe (sequential `read_exact`s), so it must never sit in a
//! `select!` that could drop it mid-frame.
//!
//! Reconnect-with-backoff is a later milestone (M4); here a dropped connection
//! fails in-flight RPCs and ends per-session streams.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use asmux::frame::{self, code, ord, Incoming};
use asmux::wire;
use planus::ReadAsRoot;

use super::BackendSpawnSpec;

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
    /// Holder ring head at report time. Reserved for the M3-exact adopt path
    /// (`attach FromCursor(consumed)`); the current adopt uses `FromEarliest`.
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

enum ClientCmd {
    Rpc {
        rpc_id: u64,
        frame: Vec<u8>,
        reply: oneshot::Sender<(u16, Vec<u8>)>,
    },
    Fire(Vec<u8>),
    Route {
        session_id: String,
        tx: mpsc::UnboundedSender<StreamEvent>,
    },
    Unroute(String),
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<(u16, Vec<u8>)>>>>;
type Routes = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<StreamEvent>>>>;

pub struct AsmuxClient {
    cmd_tx: mpsc::UnboundedSender<ClientCmd>,
    next_rpc_id: AtomicU64,
    pub instance_id: String,
    pub server_pid: i32,
}

impl AsmuxClient {
    /// Connect, run the `hello` handshake, and return a ready client.
    pub async fn connect(sock: &Path) -> Result<Arc<AsmuxClient>> {
        let stream = UnixStream::connect(sock)
            .await
            .with_context(|| format!("connecting to asmux at {}", sock.display()))?;
        let (rd, wr) = stream.into_split();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ClientCmd>();

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let routes: Routes = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(reader_task(rd, pending.clone(), routes.clone()));
        tokio::spawn(writer_task(wr, cmd_rx, pending, routes));

        // hello MUST be the first frame.
        let hello = wire::HelloRequest {
            rpc_id: 1,
            client_pid: std::process::id() as i32,
            client_name: Some("asm-daemon".to_string()),
            protocol_min: 1,
            protocol_max: 1,
        };
        let (ordinal, body) =
            send_rpc(&cmd_tx, 1, frame::encode(ord::HELLO_REQUEST, &hello)).await?;
        if ordinal != ord::HELLO_RESPONSE {
            bail!("asmux hello rejected (ordinal {ordinal})");
        }
        let hr = wire::HelloResponseRef::read_as_root(&body)
            .map_err(|e| anyhow!("bad hello response: {e}"))?;
        let instance_id = hr.instance_id().ok().flatten().unwrap_or("").to_string();
        let server_pid = hr.server_pid().unwrap_or(0);

        Ok(Arc::new(AsmuxClient {
            cmd_tx,
            next_rpc_id: AtomicU64::new(2),
            instance_id,
            server_pid,
        }))
    }

    fn alloc(&self) -> u64 {
        self.next_rpc_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn rpc(&self, rpc_id: u64, frame: Vec<u8>) -> Result<(u16, Vec<u8>)> {
        send_rpc(&self.cmd_tx, rpc_id, frame).await
    }

    /// Create a session (idempotent on `session_id`).
    pub async fn create(&self, spec: &BackendSpawnSpec) -> Result<HolderSessionInfo> {
        let id = self.alloc();
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

    /// List all sessions the holder knows about (live + tombstones).
    pub async fn list(&self) -> Result<Vec<HolderSessionInfo>> {
        let id = self.alloc();
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

    /// Attach to a session's output stream. Returns the head cursor on success.
    pub async fn attach(
        &self,
        session_id: &str,
        mode: wire::AttachMode,
        from_cursor: u64,
    ) -> Result<u64, AttachError> {
        let id = self.alloc();
        let req = wire::AttachRequest {
            rpc_id: id,
            session_id: Some(session_id.to_string()),
            mode,
            from_cursor,
        };
        let (ordinal, body) = self
            .rpc(id, frame::encode(ord::ATTACH_REQUEST, &req))
            .await
            .map_err(AttachError::Conn)?;
        if ordinal == ord::ATTACH_RESPONSE {
            let r = wire::AttachResponseRef::read_as_root(&body)
                .map_err(|e| AttachError::Conn(anyhow!("bad attach response: {e}")))?;
            Ok(r.head_cursor().unwrap_or(0))
        } else if ordinal == ord::ERROR {
            let r = wire::ErrorRef::read_as_root(&body)
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

    // ---- fire-and-forget (no reply awaited) ----

    pub fn send_input(&self, session_id: &str, data: &[u8]) {
        let req = wire::SessionInput {
            session_id: Some(session_id.to_string()),
            data: Some(data.to_vec()),
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::SESSION_INPUT, &req)));
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) {
        let req = wire::ResizeRequest {
            rpc_id: self.alloc(),
            session_id: Some(session_id.to_string()),
            cols,
            rows,
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::RESIZE_REQUEST, &req)));
    }

    pub fn kill(&self, session_id: &str, signal: i32) {
        let req = wire::KillRequest {
            rpc_id: self.alloc(),
            session_id: Some(session_id.to_string()),
            signal,
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::KILL_REQUEST, &req)));
    }

    /// Detach the daemon from a session's stream without killing the child.
    /// Reserved for M4 (clean detach / re-attach with backoff).
    #[allow(dead_code)]
    pub fn detach(&self, session_id: &str) {
        let req = wire::DetachRequest {
            rpc_id: self.alloc(),
            session_id: Some(session_id.to_string()),
        };
        let _ = self
            .cmd_tx
            .send(ClientCmd::Fire(frame::encode(ord::DETACH_REQUEST, &req)));
        self.unroute(session_id);
    }

    /// Register a per-session output route and return its receiver.
    pub fn route(&self, session_id: &str) -> mpsc::UnboundedReceiver<StreamEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = self.cmd_tx.send(ClientCmd::Route {
            session_id: session_id.to_string(),
            tx,
        });
        rx
    }

    pub fn unroute(&self, session_id: &str) {
        let _ = self.cmd_tx.send(ClientCmd::Unroute(session_id.to_string()));
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

async fn writer_task(
    mut wr: OwnedWriteHalf,
    mut cmd_rx: mpsc::UnboundedReceiver<ClientCmd>,
    pending: Pending,
    routes: Routes,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            ClientCmd::Rpc {
                rpc_id,
                frame,
                reply,
            } => {
                pending.lock().insert(rpc_id, reply);
                if wr.write_all(&frame).await.is_err() {
                    break;
                }
            }
            ClientCmd::Fire(frame) => {
                if wr.write_all(&frame).await.is_err() {
                    break;
                }
            }
            ClientCmd::Route { session_id, tx } => {
                routes.lock().insert(session_id, tx);
            }
            ClientCmd::Unroute(session_id) => {
                routes.lock().remove(&session_id);
            }
        }
    }
    let _ = wr.shutdown().await;
}

async fn reader_task(mut rd: OwnedReadHalf, pending: Pending, routes: Routes) {
    loop {
        match frame::read_frame(&mut rd).await {
            Ok(Incoming::Eof) => break,
            Ok(Incoming::Frame { ordinal, body }) => route_frame(ordinal, &body, &pending, &routes),
            Err(_) => break,
        }
    }
    // Connection gone: fail in-flight RPCs fast and end per-session streams.
    pending.lock().clear();
    routes.lock().clear();
}

fn route_frame(ordinal: u16, body: &[u8], pending: &Pending, routes: &Routes) {
    match ordinal {
        ord::SESSION_OUTPUT => {
            if let Ok(r) = wire::SessionOutputRef::read_as_root(body) {
                let sid = r.session_id().ok().flatten().unwrap_or("");
                let cursor = r.head_cursor().unwrap_or(0);
                let data = r.data().ok().flatten().unwrap_or(&[]).to_vec();
                if let Some(tx) = routes.lock().get(sid) {
                    let _ = tx.send(StreamEvent::Output { data, cursor });
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
                if let Some(tx) = routes.lock().get(sid) {
                    let _ = tx.send(ev);
                }
            }
        }
        ord::SESSION_DETACHED => {
            if let Ok(r) = wire::SessionDetachedRef::read_as_root(body) {
                let sid = r.session_id().ok().flatten().unwrap_or("");
                let reason = r.reason().map(|x| x as i8).unwrap_or(-1);
                if let Some(tx) = routes.lock().get(sid) {
                    let _ = tx.send(StreamEvent::Detached { reason });
                }
            }
        }
        ord::HEARTBEAT => {}
        _ => {
            if let Some(rid) = reply_rpc_id(ordinal, body) {
                if let Some(tx) = pending.lock().remove(&rid) {
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
