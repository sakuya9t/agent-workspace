//! Wire framing: `u32 length(BE) | u8 tag | u16 ordinal(BE) | FlatBuffers body`.
//!
//! `length` counts `tag + ordinal + body` (not itself). `tag = 0x00` selects
//! the FlatBuffers encoding — the only one defined; other tags are reserved for
//! a future frame shape and rejected here. The `ordinal` names the message and
//! fixes the body's FlatBuffers root type (there is no union). See
//! `docs/asmux-protocol.md` → Framing.

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::MAX_FRAME_BYTES;

/// The only defined frame tag: a FlatBuffers body.
pub const TAG_FLATBUFFERS: u8 = 0x00;

/// Message ordinals (the frozen ordinal catalog).
pub mod ord {
    pub const HELLO_REQUEST: u16 = 0;
    pub const HELLO_RESPONSE: u16 = 1;
    pub const CREATE_REQUEST: u16 = 2;
    pub const CREATE_RESPONSE: u16 = 3;
    pub const KILL_REQUEST: u16 = 4;
    pub const KILL_RESPONSE: u16 = 5;
    pub const PURGE_REQUEST: u16 = 6;
    pub const PURGE_RESPONSE: u16 = 7;
    pub const LIST_REQUEST: u16 = 8;
    pub const LIST_RESPONSE: u16 = 9;
    pub const UPDATE_METADATA_REQUEST: u16 = 10;
    pub const UPDATE_METADATA_RESPONSE: u16 = 11;
    pub const RESIZE_REQUEST: u16 = 12;
    pub const RESIZE_RESPONSE: u16 = 13;
    pub const READ_BUFFER_REQUEST: u16 = 14;
    pub const READ_BUFFER_RESPONSE: u16 = 15;
    pub const ATTACH_REQUEST: u16 = 16;
    pub const ATTACH_RESPONSE: u16 = 17;
    pub const DETACH_REQUEST: u16 = 20;
    pub const DETACH_RESPONSE: u16 = 21;
    pub const SESSION_EXITED: u16 = 100;
    pub const SESSION_DETACHED: u16 = 101;
    pub const ERROR: u16 = 200;
    pub const SESSION_INPUT: u16 = 300;
    pub const SESSION_OUTPUT: u16 = 301;
    pub const HEARTBEAT: u16 = 400;
}

/// Authoritative machine error codes (the `Error.code` field). Append-only.
pub mod code {
    pub const UNKNOWN_SESSION: u32 = 1;
    pub const SESSION_NOT_ALIVE: u32 = 2;
    pub const SESSION_ALIVE: u32 = 3;
    pub const BUFFER_GAP: u32 = 4;
    pub const INVALID_ARGUMENT: u32 = 5;
    pub const SPAWN_FAILED: u32 = 6;
    pub const ALLOC_FAILED: u32 = 7;
    pub const CAPACITY_OUT_OF_RANGE: u32 = 8;
    pub const PROTOCOL_MISMATCH: u32 = 9;
    pub const NOT_ATTACHED: u32 = 10;
    pub const FRAME_TOO_LARGE: u32 = 11;
    pub const INTERNAL: u32 = 12;
    pub const SESSION_EXISTS: u32 = 13;
    pub const INPUT_OVERFLOW: u32 = 14;
    pub const MEMORY_LIMIT: u32 = 15;
}

/// Outcome of reading one frame off a connection.
pub enum Incoming {
    /// A well-formed frame: its message ordinal and the FlatBuffers body bytes.
    Frame { ordinal: u16, body: Vec<u8> },
    /// The peer closed the connection cleanly at a frame boundary.
    Eof,
}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    /// `length` exceeded [`MAX_FRAME_BYTES`]; the server sends `FRAME_TOO_LARGE`
    /// (`rpc_id = 0`) best-effort, then closes.
    TooLarge,
    /// A non-`0x00` tag or a `length < 3`: a malformed/foreign envelope.
    Malformed,
    /// Transport error (partial read, reset).
    Io(std::io::Error),
}

impl From<std::io::Error> for FrameError {
    fn from(e: std::io::Error) -> Self {
        FrameError::Io(e)
    }
}

/// Read exactly one frame. Returns [`Incoming::Eof`] if the peer closes at a
/// clean boundary (no bytes of a new length prefix).
pub async fn read_frame<R>(reader: &mut R) -> Result<Incoming, FrameError>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(Incoming::Eof),
        Err(e) => return Err(FrameError::Io(e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    // length counts tag(1) + ordinal(2) + body; a valid frame is >= 3.
    let body_len = match len.checked_sub(3) {
        Some(n) => n,
        None => return Err(FrameError::Malformed),
    };

    let mut head = [0u8; 3];
    reader.read_exact(&mut head).await?;
    let tag = match head.first() {
        Some(&t) => t,
        None => return Err(FrameError::Malformed),
    };
    if tag != TAG_FLATBUFFERS {
        return Err(FrameError::Malformed);
    }
    let ordinal = match (head.get(1), head.get(2)) {
        (Some(&hi), Some(&lo)) => u16::from_be_bytes([hi, lo]),
        _ => return Err(FrameError::Malformed),
    };

    let mut body = vec![0u8; body_len];
    if body_len > 0 {
        reader.read_exact(&mut body).await?;
    }
    Ok(Incoming::Frame { ordinal, body })
}

/// Wrap already-serialized FlatBuffers `body` bytes in a frame with `ordinal`.
pub fn frame_body(ordinal: u16, body: &[u8]) -> Vec<u8> {
    // length = tag(1) + ordinal(2) + body
    let len = body.len().saturating_add(3);
    let mut out = Vec::with_capacity(len.saturating_add(4));
    out.extend_from_slice(&(len as u32).to_be_bytes());
    out.push(TAG_FLATBUFFERS);
    out.extend_from_slice(&ordinal.to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// Serialize a FlatBuffers root table and wrap it in a frame with `ordinal`.
///
/// `T` is the table type; the caller passes `&value`. Callers must pass the
/// ordinal that matches `T` (the ordinal catalog is the source of truth).
pub fn encode<T>(ordinal: u16, root: impl planus::WriteAsOffset<T>) -> Vec<u8> {
    let mut builder = planus::Builder::new();
    let body = builder.finish(root, None);
    frame_body(ordinal, body)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;
    use crate::wire;
    use planus::ReadAsRoot;

    #[tokio::test]
    async fn round_trips_a_hello_frame() {
        let hello = wire::HelloResponse {
            rpc_id: 7,
            server_pid: 123,
            binary_sha256: Some("abc".into()),
            protocol: 1,
            session_count: 2,
            started_at_unix_ms: 999,
            instance_id: Some("iid".into()),
        };
        let frame = encode(ord::HELLO_RESPONSE, &hello);

        // Length prefix is big-endian and counts everything after itself.
        let declared = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(declared, frame.len() - 4);
        assert_eq!(frame[4], TAG_FLATBUFFERS);
        assert_eq!(u16::from_be_bytes([frame[5], frame[6]]), ord::HELLO_RESPONSE);

        // Read it back through the async framer.
        let mut cursor = std::io::Cursor::new(frame);
        let got = read_frame(&mut cursor).await.expect("read");
        match got {
            Incoming::Frame { ordinal, body } => {
                assert_eq!(ordinal, ord::HELLO_RESPONSE);
                let r = wire::HelloResponseRef::read_as_root(&body).expect("parse");
                assert_eq!(r.rpc_id().unwrap(), 7);
                assert_eq!(r.server_pid().unwrap(), 123);
                assert_eq!(r.instance_id().unwrap(), Some("iid"));
            }
            Incoming::Eof => panic!("unexpected eof"),
        }
    }

    #[tokio::test]
    async fn clean_eof_at_boundary() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        match read_frame(&mut cursor).await.expect("read") {
            Incoming::Eof => {}
            Incoming::Frame { .. } => panic!("expected eof"),
        }
    }

    #[tokio::test]
    async fn rejects_oversized_length() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(u32::MAX).to_be_bytes());
        let mut cursor = std::io::Cursor::new(buf);
        match read_frame(&mut cursor).await {
            Err(FrameError::TooLarge) => {}
            _ => panic!("expected TooLarge"),
        }
    }
}
