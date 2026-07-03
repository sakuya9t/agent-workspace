//! The out-of-process holder backend: sessions live in `asmux` and survive
//! daemon restarts. This implements the same `SessionBackend`/`BackendSession`
//! traits as the native backend, so the session engine, monitor, WS API, and
//! summaries are all unchanged.
//!
//! The `vt100` emulator stays in the daemon (never in asmux): a per-session
//! **drain task** pulls raw `SessionOutput` off the asmux client, feeds the
//! emulator, broadcasts to attached clients, and persists to the cold event log.
//!
//! Sync trait methods that need an RPC bridge to the async client via
//! `block_in_place` + the current runtime handle (the daemon runs a multi-thread
//! runtime, so this never starves it).

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, watch};

use asmux::wire;

use super::asmux_client::{AsmuxClient, AttachError, StreamEvent};
use super::{
    BackendSession, BackendSpawnSpec, BackendStatus, HolderEntry, SessionBackend, Snapshot,
};
use crate::db::{Db, EventMsg, EventSink};
use crate::util::now_millis;

const BROADCAST_CAP: usize = 2048;
const SCROLLBACK: usize = 2000;

/// A backend whose sessions are held by an out-of-process `asmux`.
pub struct SidecarBackend {
    client: Arc<AsmuxClient>,
    events: EventSink,
    db: Db,
}

impl SidecarBackend {
    pub fn new(client: Arc<AsmuxClient>, events: EventSink, db: Db) -> Self {
        Self { client, events, db }
    }

    /// The holder instance identity (proves "same holder I adopted before").
    /// Reserved for M4 soft-reboot / holder-drift detection.
    #[allow(dead_code)]
    pub fn instance_id(&self) -> &str {
        &self.client.instance_id
    }
}

impl SessionBackend for SidecarBackend {
    fn id(&self) -> &'static str {
        "asmux-sidecar"
    }

    fn keep_sessions_on_shutdown(&self) -> bool {
        true
    }

    fn create(&self, spec: BackendSpawnSpec) -> Result<Arc<dyn BackendSession>> {
        let client = self.client.clone();
        let events = self.events.clone();
        let db = self.db.clone();
        let session_id = spec.session_id.clone();
        let (cols, rows) = (spec.cols, spec.rows);

        let session = block_on(async move {
            client.create(&spec).await?;
            let rx = client.route(&session_id);
            match client
                .attach(&session_id, wire::AttachMode::FromEarliest, 0)
                .await
            {
                Ok(_head) => {}
                Err(AttachError::Gap { earliest }) => {
                    // FromEarliest never gaps; log defensively if it ever does.
                    tracing::warn!(session = %session_id, earliest, "unexpected gap attaching fresh session");
                }
                Err(AttachError::Code(c)) => bail!("asmux attach failed (code {c})"),
                Err(AttachError::Conn(e)) => return Err(e),
            }
            // Fresh session: persist everything (persist_from = 0), seq from 0.
            Ok(SidecarSession::spawn(
                session_id, cols, rows, client, events, db, rx, 0, 0,
            ))
        })?;
        Ok(session)
    }

    fn holder_list(&self) -> Result<Vec<HolderEntry>> {
        let client = self.client.clone();
        let infos = block_on(async move { client.list().await })?;
        Ok(infos
            .into_iter()
            .map(|i| HolderEntry {
                id: i.id,
                alive: i.alive,
                exit_code: i.exit_code,
                exit_signal: i.exit_signal,
            })
            .collect())
    }

    fn adopt(&self, session_id: &str, rows: u16, cols: u16) -> Result<Option<Arc<dyn BackendSession>>> {
        let client = self.client.clone();
        let events = self.events.clone();
        let db = self.db.clone();
        // Continue the persisted event sequence from where the pre-restart daemon
        // left off, so new live output appends without colliding.
        let last_seq = db
            .get_session(session_id)?
            .map(|s| s.last_event_seq)
            .unwrap_or(0);
        let sid = session_id.to_string();

        let session = block_on(async move {
            let rx = client.route(&sid);
            // Reconstruct the screen by replaying the holder ring into a fresh
            // emulator. `head` is the attach boundary: bytes at/under it are
            // replay (already in cold history — feed the emulator but don't
            // re-persist); bytes beyond it are genuinely new (persist).
            let head = match client
                .attach(&sid, wire::AttachMode::FromEarliest, 0)
                .await
            {
                Ok(h) => h,
                Err(AttachError::Gap { earliest }) => {
                    tracing::warn!(session = %sid, earliest, "unexpected gap adopting session");
                    0
                }
                Err(AttachError::Code(c)) => {
                    tracing::warn!(session = %sid, code = c, "cannot adopt session");
                    client.unroute(&sid);
                    return Ok(None);
                }
                Err(AttachError::Conn(e)) => return Err(e),
            };
            let s = SidecarSession::spawn(
                sid, cols, rows, client, events, db, rx, head, last_seq,
            );
            Ok(Some(s))
        })?;
        Ok(session.map(|s| s as Arc<dyn BackendSession>))
    }
}

/// One holder-backed session; the daemon-side view (emulator + broadcast).
struct SidecarSession {
    session_id: String,
    client: Arc<AsmuxClient>,
    parser: Arc<Mutex<vt100::Parser>>,
    tx: broadcast::Sender<Arc<[u8]>>,
    status_rx: watch::Receiver<BackendStatus>,
    seq: Arc<AtomicU64>,
}

impl SidecarSession {
    #[allow(clippy::too_many_arguments)]
    fn spawn(
        session_id: String,
        cols: u16,
        rows: u16,
        client: Arc<AsmuxClient>,
        events: EventSink,
        db: Db,
        rx: mpsc::UnboundedReceiver<StreamEvent>,
        persist_from: u64,
        seq_start: u64,
    ) -> Arc<dyn BackendSession> {
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK)));
        let (tx, _keep) = broadcast::channel::<Arc<[u8]>>(BROADCAST_CAP);
        let (status_tx, status_rx) = watch::channel(BackendStatus::Running);
        let seq = Arc::new(AtomicU64::new(seq_start));

        let session = Arc::new(SidecarSession {
            session_id: session_id.clone(),
            client,
            parser: parser.clone(),
            tx: tx.clone(),
            status_rx,
            seq: seq.clone(),
        });

        tokio::spawn(drain_loop(
            session_id, parser, tx, events, db, seq, status_tx, rx, persist_from,
        ));

        session
    }

    fn build_snapshot(&self, parser: &vt100::Parser) -> Snapshot {
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        let repaint: Arc<[u8]> = Arc::from(screen.contents_formatted().into_boxed_slice());
        Snapshot {
            rows,
            cols,
            repaint,
            last_seq: self.seq.load(Ordering::SeqCst),
        }
    }
}

impl BackendSession for SidecarSession {
    fn attach(&self) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>) {
        let mut parser = self.parser.lock();
        let (rows, cols) = parser.screen().size();
        // Attach repaints include scrollback history so the client can scroll
        // up to output from before it attached.
        let repaint: Arc<[u8]> =
            Arc::from(super::repaint_with_history(&mut parser).into_boxed_slice());
        let snap = Snapshot {
            rows,
            cols,
            repaint,
            last_seq: self.seq.load(Ordering::SeqCst),
        };
        let rx = self.tx.subscribe();
        drop(parser);
        (snap, rx)
    }

    fn snapshot(&self) -> Snapshot {
        let parser = self.parser.lock();
        self.build_snapshot(&parser)
    }

    fn send_input(&self, data: &[u8]) -> Result<()> {
        self.client.send_input(&self.session_id, data);
        Ok(())
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.client.resize(&self.session_id, cols, rows);
        self.parser.lock().set_size(rows, cols);
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        // Terminate the child in the holder (0 => platform default terminate).
        self.client.kill(&self.session_id, 0);
        Ok(())
    }

    fn watch_status(&self) -> watch::Receiver<BackendStatus> {
        self.status_rx.clone()
    }

    fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

/// Pull raw output off the asmux client; feed the emulator, broadcast, persist.
#[allow(clippy::too_many_arguments)]
async fn drain_loop(
    session_id: String,
    parser: Arc<Mutex<vt100::Parser>>,
    tx: broadcast::Sender<Arc<[u8]>>,
    events: EventSink,
    db: Db,
    seq: Arc<AtomicU64>,
    status_tx: watch::Sender<BackendStatus>,
    mut rx: mpsc::UnboundedReceiver<StreamEvent>,
    persist_from: u64,
) {
    let mut parser_ok = true;
    let mut last_cursor_write = 0i64;

    while let Some(ev) = rx.recv().await {
        match ev {
            StreamEvent::Output { data, cursor } => {
                // Feed the emulator (isolate a parser panic to this session).
                {
                    let mut p = parser.lock();
                    if parser_ok
                        && std::panic::catch_unwind(AssertUnwindSafe(|| p.process(&data))).is_err()
                    {
                        parser_ok = false;
                        tracing::error!(session = %session_id, "terminal parser panicked; snapshots frozen for this session");
                    }
                }
                let arc: Arc<[u8]> = Arc::from(data.clone().into_boxed_slice());
                let _ = tx.send(arc);

                // Persist only genuinely-new bytes (replay past `persist_from`
                // is already in cold history). This is also what keeps adopt from
                // duplicating history.
                if cursor > persist_from {
                    let s = seq.fetch_add(1, Ordering::SeqCst) + 1;
                    events.send(EventMsg {
                        session_id: session_id.clone(),
                        seq: s,
                        ts_ms: now_millis(),
                        stream: 0,
                        bytes: data,
                    });
                    let now = now_millis();
                    if now - last_cursor_write >= 400 {
                        last_cursor_write = now;
                        let _ = db.set_backend_cursor(&session_id, cursor);
                    }
                }
            }
            StreamEvent::Exited { code, signal } => {
                let status = if signal != 0 {
                    BackendStatus::Failed(format!("signalled ({signal})"))
                } else if code == 0 {
                    BackendStatus::Exited(0)
                } else {
                    BackendStatus::Exited(code)
                };
                let _ = status_tx.send(status);
                break;
            }
            StreamEvent::Detached { reason } => {
                // Superseded / backpressure / server-shutdown. Robust re-attach
                // with backoff is M4; for now stop draining this stream.
                tracing::warn!(session = %session_id, reason, "asmux detached this session's stream");
                break;
            }
        }
    }
}

/// Run an async block to completion from a sync context on the current
/// multi-thread runtime without starving it.
fn block_on<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::task::block_in_place(|| Handle::current().block_on(fut))
}
