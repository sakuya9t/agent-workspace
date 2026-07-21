use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::Notify;
use tokio::time::{interval, MissedTickBehavior};

use super::AppState;

/// WebSocket close code sent to a client that was superseded by another one.
/// The client uses it to distinguish a takeover (don't reconnect) from a
/// transient drop (do reconnect).
pub const CLOSE_SUPERSEDED: u16 = 4001;

/// How often the daemon pings an attached client to prove the socket is still
/// live. Browsers auto-answer pings with pongs, so a healthy client keeps its
/// attachment fresh without any app-level cooperation.
const PING_INTERVAL: Duration = Duration::from_secs(15);

/// Release the attachment once this long passes with no frame at all from the
/// client (not even a pong). Without it, a client that slept, backgrounded, or
/// dropped its socket without a clean close leaves the session reading as
/// "in use" until OS-level TCP keepalive reaps it — on the order of hours.
const IDLE_TIMEOUT: Duration = Duration::from_secs(40);

/// Tracks the single live attacher per session (single-attacher with takeover):
/// a new live attach supersedes the previous one, which is signalled to close.
/// This mirrors the product rule "one session, one active client" at the
/// daemon↔client boundary (asmux enforces the same at its own layer).
#[derive(Default)]
pub struct Attachments {
    map: Mutex<HashMap<String, Attach>>,
    next: AtomicU64,
}

struct Attach {
    conn_id: u64,
    cancel: Arc<Notify>,
}

impl Attachments {
    pub fn new() -> Arc<Self> {
        Arc::new(Attachments {
            map: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        })
    }

    /// Is any client currently attached (live) to this session?
    pub fn is_attached(&self, session_id: &str) -> bool {
        self.map.lock().contains_key(session_id)
    }

    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Install `conn_id` as the sole attacher, returning the superseded one's
    /// cancel handle (to notify) if there was one.
    fn attach(&self, session_id: &str, conn_id: u64, cancel: Arc<Notify>) -> Option<Arc<Notify>> {
        self.map
            .lock()
            .insert(session_id.to_string(), Attach { conn_id, cancel })
            .map(|a| a.cancel)
    }

    /// Drop this connection's attachment iff it is still the current one (a
    /// later takeover by another connection must not be cleared by this one).
    fn release(&self, session_id: &str, conn_id: u64) {
        let mut m = self.map.lock();
        if m.get(session_id).map(|a| a.conn_id) == Some(conn_id) {
            m.remove(session_id);
        }
    }
}

/// Control messages from the client. Terminal input arrives either as a
/// binary frame (raw bytes) or as a JSON `{"t":"i","d":"..."}` text frame.
#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
enum ClientMsg {
    #[serde(rename = "i")]
    Input { d: String },
    #[serde(rename = "r")]
    Resize { rows: u16, cols: u16 },
}

pub async fn stream(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle(socket, id, state))
}

async fn handle(socket: WebSocket, id: String, state: AppState) {
    match state.manager.live_handle(&id) {
        Some(handle) => handle_live(socket, id, state, handle).await,
        None => handle_history(socket, id, state).await,
    }
}

/// Live session: snapshot repaint, then bidirectional streaming. Enforces
/// single-attacher-with-takeover — a new attach supersedes the previous client,
/// which is closed with [`CLOSE_SUPERSEDED`].
async fn handle_live(
    socket: WebSocket,
    id: String,
    state: AppState,
    handle: Arc<dyn crate::backend::BackendSession>,
) {
    // Acknowledge attention as soon as a client actually attaches.
    let _ = state.manager.acknowledge_attention(&id);

    // Register as the sole attacher; supersede any previous one.
    let conn_id = state.attachments.next_id();
    let cancel = Arc::new(Notify::new());
    let superseded = state.attachments.attach(&id, conn_id, cancel.clone());
    // The "in use" badge is driven by this attachment, so when it is wrong the
    // only way to tell a late close from a socket that died some other way is a
    // record of when it was taken and why it was given back. Cheap: a handful of
    // lines per attach, and `asm_daemon=debug` is on by default.
    tracing::debug!(
        session = %id,
        conn = conn_id,
        took_over = superseded.is_some(),
        "ws attach"
    );
    if let Some(prev) = superseded {
        prev.notify_one();
    }

    let (snapshot, mut out_rx) = handle.attach();
    let (mut sender, mut receiver) = socket.split();

    // Liveness clock for this attachment. The inbound loop stamps `last_seen`
    // (ms since `start`) on every frame the client sends; the outbound task
    // pings on an interval and tears the socket down once `IDLE_TIMEOUT` passes
    // with no frame. That teardown is what releases the attachment when a client
    // vanishes without a clean close — otherwise the session reads "in use"
    // until TCP keepalive reaps the dead socket hours later.
    let start = Instant::now();
    let last_seen = Arc::new(AtomicU64::new(0));

    // Prime the client with the current terminal screen (ANSI repaint).
    if sender
        .send(Message::Binary(snapshot.repaint.to_vec()))
        .await
        .is_err()
    {
        state.attachments.release(&id, conn_id);
        tracing::debug!(
            session = %id,
            conn = conn_id,
            reason = "snapshot send failed",
            held_ms = start.elapsed().as_millis() as u64,
            idle_ms = 0,
            "ws release"
        );
        return;
    }

    // Outbound: forward live output; on lag, resend a fresh snapshot; on
    // takeover, close with the superseded code so the client won't reconnect.
    let handle_out = handle.clone();
    let cancel_out = cancel.clone();
    let last_seen_out = last_seen.clone();
    // Returns why it stopped, so the release log can name the cause rather than
    // just the fact.
    let mut outbound = tokio::spawn(async move {
        let mut ping = interval(PING_INTERVAL);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ping.tick().await; // interval's first tick is immediate — skip it
        loop {
            tokio::select! {
                biased;
                _ = cancel_out.notified() => {
                    let _ = sender
                        .send(Message::Close(Some(CloseFrame {
                            code: CLOSE_SUPERSEDED,
                            reason: "superseded by another client".into(),
                        })))
                        .await;
                    return "superseded";
                }
                _ = ping.tick() => {
                    // A live client refreshes `last_seen` (browsers auto-pong our
                    // ping; input counts too). If nothing has for IDLE_TIMEOUT,
                    // treat the client as gone and drop the socket so the inbound
                    // loop below releases the attachment.
                    let idle = start.elapsed().saturating_sub(Duration::from_millis(
                        last_seen_out.load(Ordering::Relaxed),
                    ));
                    if idle >= IDLE_TIMEOUT {
                        return "idle timeout";
                    }
                    if sender.send(Message::Ping(Vec::new())).await.is_err() {
                        return "ping send failed";
                    }
                }
                recv = out_rx.recv() => {
                    match recv {
                        Ok(bytes) => {
                            if sender.send(Message::Binary(bytes.to_vec())).await.is_err() {
                                return "output send failed";
                            }
                        }
                        Err(RecvError::Lagged(_)) => {
                            let snap = handle_out.snapshot();
                            if sender
                                .send(Message::Binary(snap.repaint.to_vec()))
                                .await
                                .is_err()
                            {
                                return "repaint send failed";
                            }
                        }
                        Err(RecvError::Closed) => return "session output closed",
                    }
                }
            }
        }
    });

    // Inbound: terminal input and resize from this client.
    let reason: &'static str = loop {
        tokio::select! {
            done = &mut outbound => break match done {
                Ok(r) => r,
                Err(_) => "outbound task died",
            },
            msg = receiver.next() => {
                // Any frame (input, resize, or an auto-pong) proves the client
                // is still there and refreshes the idle deadline above.
                last_seen.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        let _ = handle.send_input(&b);
                        state.manager.note_interaction(&id, &b);
                    }
                    Some(Ok(Message::Text(t))) => {
                        match serde_json::from_str::<ClientMsg>(&t) {
                            Ok(ClientMsg::Input { d }) => {
                                let _ = handle.send_input(d.as_bytes());
                                state.manager.note_interaction(&id, d.as_bytes());
                            }
                            Ok(ClientMsg::Resize { rows, cols }) => {
                                let _ = state.manager.resize_session(&id, rows, cols);
                            }
                            Err(_) => { /* ignore malformed control frames */ }
                        }
                    }
                    // Split apart deliberately: a close frame means the client
                    // said goodbye, a bare stream end or a recv error means the
                    // socket died under us. Which one it was is the whole
                    // question when a session reads "in use" after the user
                    // already navigated away.
                    Some(Ok(Message::Close(_))) => break "client close frame",
                    None => break "client stream ended",
                    Some(Ok(_)) => { /* ping/pong/other */ }
                    Some(Err(e)) => {
                        tracing::debug!(session = %id, conn = conn_id, error = %e, "ws recv error");
                        break "recv error";
                    }
                }
            }
        }
    };

    outbound.abort();
    let held = start.elapsed();
    // How long the client had been silent when the socket ended. Near zero means
    // it left cleanly; a large value means it had already gone and only the
    // transport noticed later — which is the difference between "the client
    // never told us" and "we were slow to act on being told".
    //
    // Read it against `held_ms`: a client that sends nothing has no `last_seen`
    // until our first ping draws a pong, so on an attachment shorter than
    // PING_INTERVAL the two are equal by construction and `idle_ms` says
    // nothing. It only carries information once `held_ms` exceeds one ping.
    let idle_ms = held
        .saturating_sub(Duration::from_millis(last_seen.load(Ordering::Relaxed)))
        .as_millis() as u64;
    state.attachments.release(&id, conn_id);
    tracing::debug!(
        session = %id,
        conn = conn_id,
        reason,
        held_ms = held.as_millis() as u64,
        idle_ms,
        "ws release"
    );
}

/// Exited session: replay persisted output as a diagnostic history frame.
async fn handle_history(socket: WebSocket, id: String, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    match state.manager.db().read_events_after(&id, 0) {
        Ok(bytes) if !bytes.is_empty() => {
            let _ = sender.send(Message::Binary(bytes)).await;
        }
        Ok(_) => {
            let _ = sender
                .send(Message::Text(
                    "\r\n[session has no recorded output]\r\n".into(),
                ))
                .await;
        }
        Err(e) => {
            let _ = sender
                .send(Message::Text(format!("\r\n[history unavailable: {e}]\r\n")))
                .await;
        }
    }

    // Keep the socket open until the client leaves so the frame is delivered.
    while let Some(Ok(msg)) = receiver.next().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }
}
