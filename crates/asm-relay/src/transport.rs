//! WebSocket ↔ byte-duplex adapter.
//!
//! Both sides of a dial-out data stream need to treat a WebSocket as a raw
//! byte pipe: the relay runs an HTTP/1.1 client (`hyper`) over it to reach the
//! daemon; the node splices it to a local TCP socket. This adapter presents any
//! WebSocket — axum's server-side `WebSocket` or tokio-tungstenite's
//! client-side `WebSocketStream` — as [`AsyncRead`] + [`AsyncWrite`].
//!
//! Binary frames carry the bytes. Ping/pong/text frames are transparently
//! skipped on read. A close frame or end-of-stream reads as EOF. Each
//! `poll_write` emits exactly one binary frame (hyper writes in reasonable
//! chunks, so this is fine).

use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Classification of an inbound WS frame for byte-stream purposes.
pub enum WsPayload {
    Binary(Vec<u8>),
    Ignore,
    Close,
}

/// Abstracts the two WS `Message` types so one adapter serves both directions.
pub trait WsMessage {
    fn from_bytes(data: Vec<u8>) -> Self;
    fn into_payload(self) -> WsPayload;
}

/// Presents a WebSocket as an `AsyncRead + AsyncWrite` byte duplex.
pub struct WsByteStream<S> {
    inner: S,
    read_rem: Vec<u8>,
    read_pos: usize,
}

impl<S> WsByteStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            read_rem: Vec::new(),
            read_pos: 0,
        }
    }
}

fn io_err<E>(_e: E) -> io::Error {
    io::Error::other("websocket transport error")
}

impl<S, M, E> AsyncRead for WsByteStream<S>
where
    S: Stream<Item = Result<M, E>> + Sink<M> + Unpin,
    M: WsMessage,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if this.read_pos < this.read_rem.len() {
                let n = std::cmp::min(buf.remaining(), this.read_rem.len() - this.read_pos);
                buf.put_slice(&this.read_rem[this.read_pos..this.read_pos + n]);
                this.read_pos += n;
                return Poll::Ready(Ok(()));
            }
            match ready!(this.inner.poll_next_unpin(cx)) {
                Some(Ok(msg)) => match msg.into_payload() {
                    WsPayload::Binary(data) => {
                        if data.is_empty() {
                            continue;
                        }
                        this.read_rem = data;
                        this.read_pos = 0;
                        // loop around to serve from the fresh buffer
                    }
                    WsPayload::Ignore => continue,
                    WsPayload::Close => return Poll::Ready(Ok(())),
                },
                Some(Err(e)) => return Poll::Ready(Err(io_err(e))),
                None => return Poll::Ready(Ok(())),
            }
        }
    }
}

impl<S, M, E> AsyncWrite for WsByteStream<S>
where
    S: Stream<Item = Result<M, E>> + Sink<M> + Unpin,
    M: WsMessage,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        ready!(this.inner.poll_ready_unpin(cx)).map_err(io_err)?;
        this.inner
            .start_send_unpin(M::from_bytes(buf.to_vec()))
            .map_err(io_err)?;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.inner.poll_flush_unpin(cx).map_err(io_err)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.inner.poll_close_unpin(cx).map_err(io_err)
    }
}

// ---- Message impls: axum (relay/server side) and tungstenite (node side) ----

impl WsMessage for axum::extract::ws::Message {
    fn from_bytes(data: Vec<u8>) -> Self {
        axum::extract::ws::Message::Binary(data)
    }
    fn into_payload(self) -> WsPayload {
        match self {
            axum::extract::ws::Message::Binary(d) => WsPayload::Binary(d),
            axum::extract::ws::Message::Close(_) => WsPayload::Close,
            _ => WsPayload::Ignore,
        }
    }
}

impl WsMessage for tokio_tungstenite::tungstenite::Message {
    fn from_bytes(data: Vec<u8>) -> Self {
        tokio_tungstenite::tungstenite::Message::Binary(data)
    }
    fn into_payload(self) -> WsPayload {
        match self {
            tokio_tungstenite::tungstenite::Message::Binary(d) => WsPayload::Binary(d),
            tokio_tungstenite::tungstenite::Message::Close(_) => WsPayload::Close,
            _ => WsPayload::Ignore,
        }
    }
}
