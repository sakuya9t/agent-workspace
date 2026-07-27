//! The out-of-process holder backend: sessions live in `asmux` and survive
//! daemon restarts. This implements the same `SessionBackend`/`BackendSession`
//! traits as the native backend, so the session engine, monitor, WS API, and
//! summaries are all unchanged.
//!
//! The `vt100` emulator stays in the daemon (never in asmux): a per-session
//! **drain task** pulls raw `SessionOutput` off the asmux client, feeds the
//! emulator, broadcasts to attached clients, and persists to the cold event log.
//! The drain task lives as long as the *session*, not the connection: a socket
//! drop is invisible to it (the supervisor re-attaches the route), and a
//! per-session backpressure eviction is recovered in place by re-attaching.
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

use super::asmux_client::{AttachError, Holder, StreamEvent};
use super::{
    BackendSession, BackendSpawnSpec, BackendStatus, HistoryRing, HolderEntry, SessionBackend,
    Snapshot, StreamEnd, HISTORY_RING_BYTES,
};
use crate::db::{Db, EventMsg, EventSink};
use crate::util::now_millis;

const BROADCAST_CAP: usize = 2048;
const SCROLLBACK: usize = 2000;
/// `DetachReason::Backpressure` — the only detach reason we recover from in
/// place (resync via `attach FromCursor`); the rest are terminal.
const DETACH_BACKPRESSURE: i8 = 2;

/// A backend whose sessions are held by an out-of-process `asmux`.
pub struct SidecarBackend {
    client: Arc<dyn Holder>,
    events: EventSink,
    db: Db,
}

impl SidecarBackend {
    pub fn new(client: Arc<dyn Holder>, events: EventSink, db: Db) -> Self {
        Self { client, events, db }
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
        let session_id = spec.session_id.clone();
        let (cols, rows) = (spec.cols, spec.rows);

        let session = block_on(async move {
            client.create(&spec).await?;
            // Fresh session: the attach below starts at the very beginning, so
            // that is also where a reconnect must resume from.
            let rx = client.route(&session_id, 0);
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
            // Fresh session: empty emulator, persist everything (persist_from = 0),
            // seq from 0.
            let (parser, history) = fresh_emulator(rows, cols);
            Ok(SidecarSession::spawn(
                session_id, client, events, rx, parser, history, 0, 0,
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
        let sid = session_id.to_string();

        // Cold-stitch adopt: the daemon persisted *every* output chunk, so cold
        // history covers `0..consumed`; only the un-drained tail `(consumed..head]`
        // is missing. Seed the emulator from cold history, then attach the ring
        // from exactly `consumed`.
        let consumed = db.get_backend_cursor(&sid)?;
        // Continue the event sequence past the true max persisted seq (not the
        // throttled `last_event_seq`, which could collide with existing rows).
        let seq_start = db.max_event_seq(&sid)?;
        let cold = db.read_events_after(&sid, 0)?;

        let session = block_on(async move {
            // Reconstruct both the screen (vt100) and the normal-buffer raw
            // scrollback (HistoryRing) from cold history.
            let (parser, history) = seed_from_cold(rows, cols, &cold);
            // Seeded with the same cursor the attach below uses. A socket drop
            // in the window before the first post-adopt chunk arrives would
            // otherwise re-attach from 0 and replay the whole ring — through the
            // emulator and history ring, not just the (already gated) persist —
            // duplicating the scrollback cold-stitch adopt just made exact.
            let rx = client.route(&sid, consumed);
            let persist_from = match client
                .attach(&sid, wire::AttachMode::FromCursor, consumed)
                .await
            {
                // Exact: cold history ends at `consumed`; the ring holds the
                // un-drained tail `(consumed..head]` (all genuinely new → persist).
                Ok(_head) => consumed,
                Err(AttachError::Gap { earliest }) => {
                    // The ring wrapped past `consumed` while the daemon was down:
                    // bytes `(consumed..earliest)` are gone from both tiers. Show a
                    // gap marker, then resync from the current ring tail. The screen
                    // is approximate (starts mid-stream) until the app repaints.
                    let lost = earliest.saturating_sub(consumed);
                    tracing::warn!(session = %sid, consumed, earliest, lost, "adopt ring gap; rendering gap marker");
                    super::render_gap_marker(&parser, &history, lost);
                    match client.attach(&sid, wire::AttachMode::FromEarliest, 0).await {
                        Ok(_) => {}
                        Err(AttachError::Conn(e)) => return Err(e),
                        Err(_) => {}
                    }
                    // We did not land where we asked: the stream now resumes at
                    // the ring tail, so that — not `consumed` — is what a
                    // reconnect must ask for. (`persist_from` stays `consumed`:
                    // everything past it is genuinely new to cold history.)
                    client.set_stream_cursor(&sid, earliest);
                    consumed
                }
                Err(AttachError::Code(c)) => {
                    tracing::warn!(session = %sid, code = c, "cannot adopt session");
                    client.unroute(&sid);
                    return Ok(None);
                }
                Err(AttachError::Conn(e)) => return Err(e),
            };
            let s = SidecarSession::spawn(
                sid, client, events, rx, parser, history, persist_from, seq_start,
            );
            Ok(Some(s))
        })?;
        Ok(session.map(|s| s as Arc<dyn BackendSession>))
    }

    fn end_session_stream(&self, id: &str, outcome: StreamEnd) {
        match outcome {
            // Drive the normal exit path through the drain (the monitor writes
            // the summary and removes it from `live`).
            StreamEnd::Exited { code, signal } => self.client.inject_exit(id, code, signal),
            // No completion record: close the drain so its monitor stops; the
            // manager then marks the row `indeterminate`.
            StreamEnd::Vanished => self.client.unroute(id),
        }
    }
}

/// One holder-backed session; the daemon-side view (emulator + broadcast).
struct SidecarSession {
    session_id: String,
    client: Arc<dyn Holder>,
    parser: Arc<Mutex<vt100::Parser>>,
    history: Arc<Mutex<HistoryRing>>,
    tx: broadcast::Sender<Arc<[u8]>>,
    status_rx: watch::Receiver<BackendStatus>,
    seq: Arc<AtomicU64>,
}

impl SidecarSession {
    /// `parser`/`history` are supplied pre-built so `create` can pass fresh ones
    /// and `adopt` can pass ones already seeded from cold history.
    #[allow(clippy::too_many_arguments)]
    fn spawn(
        session_id: String,
        client: Arc<dyn Holder>,
        events: EventSink,
        rx: mpsc::UnboundedReceiver<StreamEvent>,
        parser: Arc<Mutex<vt100::Parser>>,
        history: Arc<Mutex<HistoryRing>>,
        persist_from: u64,
        seq_start: u64,
    ) -> Arc<dyn BackendSession> {
        let (tx, _keep) = broadcast::channel::<Arc<[u8]>>(BROADCAST_CAP);
        let (status_tx, status_rx) = watch::channel(BackendStatus::Running);
        let seq = Arc::new(AtomicU64::new(seq_start));

        let session = Arc::new(SidecarSession {
            session_id: session_id.clone(),
            client: client.clone(),
            parser: parser.clone(),
            history: history.clone(),
            tx: tx.clone(),
            status_rx,
            seq: seq.clone(),
        });

        tokio::spawn(drain_loop(DrainCtx {
            session_id,
            client,
            parser,
            history,
            tx,
            events,
            seq,
            status_tx,
            rx,
            persist_from,
        }));

        session
    }
}

/// A fresh, empty emulator + raw-history ring for a new session.
fn fresh_emulator(rows: u16, cols: u16) -> (Arc<Mutex<vt100::Parser>>, Arc<Mutex<HistoryRing>>) {
    (
        Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK))),
        Arc::new(Mutex::new(HistoryRing::new(HISTORY_RING_BYTES))),
    )
}

/// Seed a fresh emulator + raw-history ring from a session's cold history so an
/// adopt reconstructs the screen exactly (up to `consumed`). The full history
/// feeds `vt100` (it self-trims to its scrollback); the `HistoryRing` is fed in
/// bounded chunks so it keeps only its byte-capped tail (for normal-buffer
/// scrollback replay). Replaying full history is a one-time adopt cost; the
/// periodic-snapshot optimization that bounds it is a Stage C follow-up.
fn seed_from_cold(
    rows: u16,
    cols: u16,
    cold: &[u8],
) -> (Arc<Mutex<vt100::Parser>>, Arc<Mutex<HistoryRing>>) {
    let (parser, history) = fresh_emulator(rows, cols);
    if !cold.is_empty() {
        {
            let mut p = parser.lock();
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| p.process(cold)));
        }
        let mut h = history.lock();
        for chunk in cold.chunks(64 * 1024) {
            h.push(Arc::from(chunk.to_vec().into_boxed_slice()));
        }
    }
    (parser, history)
}

impl BackendSession for SidecarSession {
    fn attach(&self) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>) {
        super::attach_with_history(&self.parser, &self.history, &self.tx, &self.seq)
    }

    fn snapshot(&self) -> Snapshot {
        super::snapshot_screen(&self.parser.lock(), &self.seq)
    }

    fn screen_text(&self) -> String {
        self.parser.lock().screen().contents()
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

/// Inputs to the per-session drain task, grouped to keep the spawn call legible.
struct DrainCtx {
    session_id: String,
    client: Arc<dyn Holder>,
    parser: Arc<Mutex<vt100::Parser>>,
    history: Arc<Mutex<HistoryRing>>,
    tx: broadcast::Sender<Arc<[u8]>>,
    events: EventSink,
    seq: Arc<AtomicU64>,
    status_tx: watch::Sender<BackendStatus>,
    rx: mpsc::UnboundedReceiver<StreamEvent>,
    persist_from: u64,
}

/// Pull raw output off the asmux client; feed the emulator, broadcast, persist.
/// Lives for the session's lifetime: it ends only on a real exit, a terminal
/// detach (superseded/shutdown/purged), or the route closing (`unroute`).
async fn drain_loop(ctx: DrainCtx) {
    let DrainCtx {
        session_id,
        client,
        parser,
        history,
        tx,
        events,
        seq,
        status_tx,
        mut rx,
        persist_from,
    } = ctx;
    let mut parser_ok = true;

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
                // Push the ring and broadcast under the ring lock so a
                // normal-buffer attach (which reads the ring + subscribes under
                // that lock) sees a single consistent stream.
                let arc: Arc<[u8]> = Arc::from(data.clone().into_boxed_slice());
                {
                    let mut h = history.lock();
                    h.push(arc.clone());
                    let _ = tx.send(arc);
                }

                // Persist only genuinely-new bytes (replay past `persist_from`
                // is already in cold history). This is also what keeps adopt from
                // duplicating history. The event write also advances the persisted
                // `backend_cursor` (= exact end of cold history) via `head_cursor`.
                if cursor > persist_from {
                    let s = seq.fetch_add(1, Ordering::SeqCst) + 1;
                    events.send(EventMsg {
                        session_id: session_id.clone(),
                        seq: s,
                        ts_ms: now_millis(),
                        stream: 0,
                        bytes: data,
                        head_cursor: cursor,
                    });
                }
            }
            StreamEvent::Exited { code, signal } => {
                let status = if signal != 0 {
                    BackendStatus::Failed(format!("signalled ({signal})"))
                } else {
                    BackendStatus::Exited(code)
                };
                let _ = status_tx.send(status);
                break;
            }
            StreamEvent::Detached { reason } => {
                if reason == DETACH_BACKPRESSURE {
                    // This session's stream fell behind and was evicted. Resync
                    // in place from where the route says it stopped — the same
                    // number a reconnect would resume from, asked of its one
                    // owner rather than tracked again here. A socket drop during
                    // this is fine: the supervisor re-attaches on reconnect.
                    let from = client.stream_cursor(&session_id);
                    tracing::warn!(session = %session_id, from, "asmux backpressure eviction; resyncing");
                    match client
                        .attach(&session_id, wire::AttachMode::FromCursor, from)
                        .await
                    {
                        Ok(_) => {}
                        Err(AttachError::Gap { earliest }) => {
                            tracing::warn!(session = %session_id, earliest, "backpressure resync gap; FromEarliest");
                            let _ = client
                                .attach(&session_id, wire::AttachMode::FromEarliest, 0)
                                .await;
                            client.set_stream_cursor(&session_id, earliest);
                        }
                        // A connection error here is recovered by the supervisor's
                        // reconnect + reattach; keep draining the same route.
                        Err(_) => {}
                    }
                    continue;
                }
                // Superseded / server shutdown / purged: nothing to resync.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    use async_trait::async_trait;

    use crate::domain::{AttentionState, Session, SessionStatus};
    use uuid::Uuid;

    #[derive(Clone, Copy)]
    enum AttachStep {
        Ok(u64),
        Gap(u64),
        Code(u32),
    }

    struct MockRoute {
        tx: mpsc::UnboundedSender<StreamEvent>,
        cursor: u64,
    }

    /// Scriptable holder test double. It deliberately models the two pieces the
    /// sidecar relies on independently: RPC outcomes and the asynchronous event
    /// route. This makes attach gaps, detach/resync, exits, and vanished routes
    /// deterministic without a real socket or PTY.
    #[derive(Default)]
    struct MockHolder {
        attach_steps: Mutex<VecDeque<AttachStep>>,
        attach_calls: Mutex<Vec<(String, wire::AttachMode, u64)>>,
        creates: Mutex<Vec<BackendSpawnSpec>>,
        routes: Mutex<HashMap<String, MockRoute>>,
        inputs: Mutex<Vec<(String, Vec<u8>)>>,
        resizes: Mutex<Vec<(String, u16, u16)>>,
        kills: Mutex<Vec<(String, i32)>>,
        unroutes: Mutex<Vec<String>>,
    }

    impl MockHolder {
        fn with_attach_steps(steps: impl IntoIterator<Item = AttachStep>) -> Arc<Self> {
            Arc::new(Self {
                attach_steps: Mutex::new(steps.into_iter().collect()),
                ..Self::default()
            })
        }

        fn emit(&self, session_id: &str, event: StreamEvent) {
            let mut routes = self.routes.lock();
            let route = routes.get_mut(session_id).expect("route must exist");
            if let StreamEvent::Output { cursor, .. } = &event {
                route.cursor = *cursor;
            }
            route.tx.send(event).expect("drain route must be open");
        }
    }

    #[async_trait]
    impl Holder for MockHolder {
        async fn create(&self, spec: &BackendSpawnSpec) -> Result<super::super::asmux_client::HolderSessionInfo> {
            self.creates.lock().push(spec.clone());
            Ok(super::super::asmux_client::HolderSessionInfo {
                id: spec.session_id.clone(),
                alive: true,
                exit_code: 0,
                exit_signal: 0,
                head_cursor: 0,
            })
        }

        async fn list(&self) -> Result<Vec<super::super::asmux_client::HolderSessionInfo>> {
            Ok(vec![])
        }

        async fn attach(
            &self,
            session_id: &str,
            mode: wire::AttachMode,
            from_cursor: u64,
        ) -> std::result::Result<u64, AttachError> {
            self.attach_calls
                .lock()
                .push((session_id.to_string(), mode, from_cursor));
            match self
                .attach_steps
                .lock()
                .pop_front()
                .unwrap_or(AttachStep::Ok(from_cursor))
            {
                AttachStep::Ok(head) => Ok(head),
                AttachStep::Gap(earliest) => Err(AttachError::Gap { earliest }),
                AttachStep::Code(code) => Err(AttachError::Code(code)),
            }
        }

        fn send_input(&self, session_id: &str, data: &[u8]) {
            self.inputs
                .lock()
                .push((session_id.to_string(), data.to_vec()));
        }

        fn resize(&self, session_id: &str, cols: u16, rows: u16) {
            self.resizes
                .lock()
                .push((session_id.to_string(), cols, rows));
        }

        fn kill(&self, session_id: &str, signal: i32) {
            self.kills.lock().push((session_id.to_string(), signal));
        }

        fn route(
            &self,
            session_id: &str,
            from_cursor: u64,
        ) -> mpsc::UnboundedReceiver<StreamEvent> {
            let (tx, rx) = mpsc::unbounded_channel();
            self.routes.lock().insert(
                session_id.to_string(),
                MockRoute {
                    tx,
                    cursor: from_cursor,
                },
            );
            rx
        }

        fn unroute(&self, session_id: &str) {
            self.routes.lock().remove(session_id);
            self.unroutes.lock().push(session_id.to_string());
        }

        fn set_stream_cursor(&self, session_id: &str, cursor: u64) {
            if let Some(route) = self.routes.lock().get_mut(session_id) {
                route.cursor = cursor;
            }
        }

        fn stream_cursor(&self, session_id: &str) -> u64 {
            self.routes
                .lock()
                .get(session_id)
                .map(|route| route.cursor)
                .unwrap_or(0)
        }

        fn inject_exit(&self, session_id: &str, code: i32, signal: i32) {
            if let Some(route) = self.routes.lock().get(session_id) {
                let _ = route.tx.send(StreamEvent::Exited { code, signal });
            }
        }
    }

    fn test_db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("asm-sidecar-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (Db::open(&dir.join("test.sqlite3")).unwrap(), dir)
    }

    fn seed_session(db: &Db, id: &str) {
        let now = now_millis();
        db.insert_session(&Session {
            id: id.into(),
            agent_plugin_id: "shell".into(),
            command: "sh".into(),
            args: vec![],
            env: vec![],
            working_directory: std::env::temp_dir().to_string_lossy().into_owned(),
            workspace_id: None,
            status: SessionStatus::Running,
            rows: 24,
            cols: 80,
            last_event_seq: 0,
            exit_code: None,
            attention_state: AttentionState::None,
            attention_reason: None,
            created_at: now,
            updated_at: now,
            last_activity_at: now,
            risky: false,
            agent_session_id: None,
            forked_from: None,
        })
        .unwrap();
    }

    fn spawn_spec(id: &str) -> BackendSpawnSpec {
        BackendSpawnSpec {
            session_id: id.into(),
            command: "cat".into(),
            args: vec![],
            env: vec![],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            rows: 24,
            cols: 80,
        }
    }

    async fn wait_until(mut predicate: impl FnMut() -> bool, message: &str) {
        for _ in 0..100 {
            if predicate() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {message}");
    }

    fn ring_bytes(history: &Mutex<HistoryRing>) -> Vec<u8> {
        let mut out = Vec::new();
        history.lock().extend_into(&mut out);
        out
    }

    #[test]
    fn seed_from_cold_reconstructs_screen_and_scrollback() {
        // Cold history longer than the screen: the seeded emulator shows the
        // latest screen, and the raw-history ring still carries the early output
        // (so a normal-buffer attach replays it as scrollback).
        let mut cold = Vec::new();
        for i in 0..50 {
            cold.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        cold.extend_from_slice(b"prompt> ");
        let (parser, history) = seed_from_cold(24, 80, &cold);

        let screen = parser.lock().screen().contents();
        assert!(screen.contains("prompt>"), "latest screen: {screen:?}");
        assert!(screen.contains("line 49"), "recent line on screen");

        let ring = String::from_utf8_lossy(&ring_bytes(&history)).into_owned();
        assert!(ring.contains("line 0"), "early output recoverable from ring");
        assert!(ring.contains("line 49"));
    }

    #[test]
    fn seed_from_cold_empty_history_is_blank() {
        let (parser, history) = seed_from_cold(24, 80, &[]);
        assert_eq!(parser.lock().screen().contents().trim(), "");
        assert!(ring_bytes(&history).is_empty());
    }

    #[test]
    fn gap_marker_lands_in_screen_and_ring() {
        let (parser, history) = fresh_emulator(24, 80);
        crate::backend::render_gap_marker(&parser, &history, 4096);
        assert!(parser
            .lock()
            .screen()
            .contents()
            .contains("not recorded during the restart gap"));
        let ring = String::from_utf8_lossy(&ring_bytes(&history)).into_owned();
        assert!(ring.contains("4096 bytes"), "byte count shown: {ring:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mock_holder_drives_create_output_commands_and_exit() {
        let (db, dir) = test_db();
        seed_session(&db, "fresh");
        let holder = MockHolder::with_attach_steps([AttachStep::Ok(0)]);
        let backend = SidecarBackend::new(holder.clone(), db.events(), db.clone());

        let session = backend.create(spawn_spec("fresh")).unwrap();
        assert_eq!(holder.creates.lock().len(), 1);
        assert_eq!(
            holder.attach_calls.lock().as_slice(),
            &[("fresh".into(), wire::AttachMode::FromEarliest, 0)]
        );

        holder.emit(
            "fresh",
            StreamEvent::Output {
                data: b"hello from holder\r\n".to_vec(),
                cursor: 19,
            },
        );
        wait_until(
            || session.screen_text().contains("hello from holder"),
            "mock output to reach the emulator",
        )
        .await;
        assert_eq!(session.last_seq(), 1);
        wait_until(
            || {
                db.read_events_after("fresh", 0).unwrap()
                    == b"hello from holder\r\n"
            },
            "mock output persistence",
        )
        .await;

        session.send_input(b"input").unwrap();
        session.resize(40, 120).unwrap();
        session.stop().unwrap();
        assert_eq!(holder.inputs.lock().as_slice(), &[("fresh".into(), b"input".to_vec())]);
        assert_eq!(holder.resizes.lock().as_slice(), &[("fresh".into(), 120, 40)]);
        assert_eq!(holder.kills.lock().as_slice(), &[("fresh".into(), 0)]);

        holder.emit("fresh", StreamEvent::Exited { code: 7, signal: 0 });
        let mut status = session.watch_status();
        wait_until(
            || {
                status.borrow_and_update().clone() == BackendStatus::Exited(7)
            },
            "exit status",
        )
        .await;

        drop(session);
        drop(backend);
        drop(holder);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mock_holder_drives_backpressure_gap_resync() {
        let (db, dir) = test_db();
        let holder = MockHolder::with_attach_steps([
            AttachStep::Ok(0),
            AttachStep::Gap(7),
            AttachStep::Ok(12),
        ]);
        let backend = SidecarBackend::new(holder.clone(), db.events(), db);
        let session = backend.create(spawn_spec("resync")).unwrap();

        holder.emit(
            "resync",
            StreamEvent::Detached {
                reason: DETACH_BACKPRESSURE,
            },
        );
        wait_until(
            || holder.attach_calls.lock().len() == 3,
            "backpressure reattach sequence",
        )
        .await;
        assert_eq!(
            holder.attach_calls.lock().as_slice(),
            &[
                ("resync".into(), wire::AttachMode::FromEarliest, 0),
                ("resync".into(), wire::AttachMode::FromCursor, 0),
                ("resync".into(), wire::AttachMode::FromEarliest, 0),
            ]
        );
        assert_eq!(holder.stream_cursor("resync"), 7);

        holder.emit("resync", StreamEvent::Exited { code: 0, signal: 0 });
        drop(session);
        drop(backend);
        drop(holder);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mock_holder_drives_cold_adopt_gap_and_end_stream() {
        let (db, dir) = test_db();
        seed_session(&db, "adopted");
        db.events().send(EventMsg {
            session_id: "adopted".into(),
            seq: 1,
            ts_ms: now_millis(),
            stream: 0,
            bytes: b"cold history\r\n".to_vec(),
            head_cursor: 14,
        });
        wait_until(
            || db.get_backend_cursor("adopted").unwrap() == 14,
            "cold event persistence",
        )
        .await;

        let holder =
            MockHolder::with_attach_steps([AttachStep::Gap(20), AttachStep::Ok(24)]);
        let backend = SidecarBackend::new(holder.clone(), db.events(), db);
        let session = backend
            .adopt("adopted", 24, 80)
            .unwrap()
            .expect("gap falls back to the earliest retained output");

        assert_eq!(
            holder.attach_calls.lock().as_slice(),
            &[
                ("adopted".into(), wire::AttachMode::FromCursor, 14),
                ("adopted".into(), wire::AttachMode::FromEarliest, 0),
            ]
        );
        assert_eq!(holder.stream_cursor("adopted"), 20);
        let screen = session.screen_text();
        assert!(screen.contains("cold history"), "cold screen: {screen:?}");
        assert!(
            screen.contains("not recorded during the restart gap"),
            "gap marker: {screen:?}"
        );

        backend.end_session_stream(
            "adopted",
            StreamEnd::Exited {
                code: -1,
                signal: 9,
            },
        );
        let mut status = session.watch_status();
        wait_until(
            || {
                matches!(
                    &*status.borrow_and_update(),
                    BackendStatus::Failed(reason) if reason == "signalled (9)"
                )
            },
            "synthetic holder exit",
        )
        .await;

        backend.end_session_stream("vanished", StreamEnd::Vanished);
        assert_eq!(holder.unroutes.lock().as_slice(), &["vanished".to_string()]);

        drop(session);
        drop(backend);
        drop(holder);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn attach_error_unroutes_and_declines_adoption() {
        let (db, dir) = test_db();
        seed_session(&db, "gone");
        let holder = MockHolder::with_attach_steps([AttachStep::Code(404)]);
        let backend = SidecarBackend::new(holder.clone(), db.events(), db);

        assert!(backend.adopt("gone", 24, 80).unwrap().is_none());
        assert_eq!(holder.unroutes.lock().as_slice(), &["gone".to_string()]);

        drop(backend);
        drop(holder);
        let _ = std::fs::remove_dir_all(dir);
    }
}
