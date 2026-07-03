use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::Notify;

use super::AppState;

/// WebSocket close code sent to a client that was superseded by another one.
/// The client uses it to distinguish a takeover (don't reconnect) from a
/// transient drop (do reconnect).
pub const CLOSE_SUPERSEDED: u16 = 4001;

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
    if let Some(prev) = state.attachments.attach(&id, conn_id, cancel.clone()) {
        prev.notify_one();
    }

    let (snapshot, mut out_rx) = handle.attach();
    let (mut sender, mut receiver) = socket.split();

    // Prime the client with the current terminal screen (ANSI repaint).
    if sender
        .send(Message::Binary(snapshot.repaint.to_vec()))
        .await
        .is_err()
    {
        state.attachments.release(&id, conn_id);
        return;
    }

    // Outbound: forward live output; on lag, resend a fresh snapshot; on
    // takeover, close with the superseded code so the client won't reconnect.
    let handle_out = handle.clone();
    let cancel_out = cancel.clone();
    let mut outbound = tokio::spawn(async move {
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
                    break;
                }
                recv = out_rx.recv() => {
                    match recv {
                        Ok(bytes) => {
                            if sender.send(Message::Binary(bytes.to_vec())).await.is_err() {
                                break;
                            }
                        }
                        Err(RecvError::Lagged(_)) => {
                            let snap = handle_out.snapshot();
                            if sender
                                .send(Message::Binary(snap.repaint.to_vec()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // Inbound: terminal input and resize from this client.
    loop {
        tokio::select! {
            _ = &mut outbound => break,
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        let _ = handle.send_input(&b);
                        state.manager.note_interaction(&id);
                    }
                    Some(Ok(Message::Text(t))) => {
                        match serde_json::from_str::<ClientMsg>(&t) {
                            Ok(ClientMsg::Input { d }) => {
                                let _ = handle.send_input(d.as_bytes());
                                state.manager.note_interaction(&id);
                            }
                            Ok(ClientMsg::Resize { rows, cols }) => {
                                let _ = state.manager.resize_session(&id, rows, cols);
                            }
                            Err(_) => { /* ignore malformed control frames */ }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => { /* ping/pong/other */ }
                    Some(Err(_)) => break,
                }
            }
        }
    }

    outbound.abort();
    state.attachments.release(&id, conn_id);
}

/// Exited session: replay persisted output as a diagnostic history frame.
async fn handle_history(socket: WebSocket, id: String, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    match state.manager.db.read_events_after(&id, 0) {
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
