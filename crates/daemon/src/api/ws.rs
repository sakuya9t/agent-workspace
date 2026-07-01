use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use super::AppState;

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

/// Live session: snapshot repaint, then bidirectional streaming.
async fn handle_live(
    socket: WebSocket,
    id: String,
    state: AppState,
    handle: Arc<dyn crate::backend::BackendSession>,
) {
    // Acknowledge attention as soon as a client actually attaches.
    let _ = state.manager.acknowledge_attention(&id);

    let (snapshot, mut out_rx) = handle.attach();
    let (mut sender, mut receiver) = socket.split();

    // Prime the client with the current terminal screen (ANSI repaint).
    if sender
        .send(Message::Binary(snapshot.repaint.to_vec()))
        .await
        .is_err()
    {
        return;
    }

    // Outbound: forward live output; on lag, resend a fresh snapshot.
    let handle_out = handle.clone();
    let mut outbound = tokio::spawn(async move {
        loop {
            match out_rx.recv().await {
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
    });

    // Inbound: terminal input and resize from this client.
    loop {
        tokio::select! {
            _ = &mut outbound => break,
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        let _ = handle.send_input(&b);
                    }
                    Some(Ok(Message::Text(t))) => {
                        match serde_json::from_str::<ClientMsg>(&t) {
                            Ok(ClientMsg::Input { d }) => {
                                let _ = handle.send_input(d.as_bytes());
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
