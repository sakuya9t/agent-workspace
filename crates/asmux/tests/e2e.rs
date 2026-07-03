//! End-to-end M1 verification: spin up the real asmux server on a temp socket
//! and drive it through the full session lifecycle as the client would —
//! hello → create → idempotent re-create → list → attach → input/output →
//! resize → metadata → kill → SessionExited → purge.
//!
//! Integration tests are a separate crate, so the holder's `#![deny]` never-crash
//! lints don't apply here; `unwrap`/`panic` are fine in test scaffolding.

use std::sync::Arc;

use asmux::frame::{self, ord, Incoming};
use asmux::registry::Registry;
use asmux::server::{serve, ServerCtx};
use asmux::wire;
use asmux::MEMORY_LIMIT_DEFAULT_BYTES;
use planus::ReadAsRoot;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};

async fn write_frame(wr: &mut OwnedWriteHalf, bytes: Vec<u8>) {
    wr.write_all(&bytes).await.unwrap();
}

/// Read the next non-heartbeat frame.
async fn recv(rd: &mut OwnedReadHalf) -> (u16, Vec<u8>) {
    loop {
        match frame::read_frame(rd).await.unwrap() {
            Incoming::Frame { ordinal, body } => {
                if ordinal == ord::HEARTBEAT {
                    continue;
                }
                return (ordinal, body);
            }
            Incoming::Eof => panic!("unexpected EOF"),
        }
    }
}

/// Read the next RPC response/event, skipping the async `SessionOutput` stream
/// that can interleave with it (a real client demultiplexes the socket by
/// ordinal; the ring is the source of truth for output, delivered out of band).
async fn recv_resp(rd: &mut OwnedReadHalf) -> (u16, Vec<u8>) {
    loop {
        let (ordinal, body) = recv(rd).await;
        if ordinal == ord::SESSION_OUTPUT {
            continue;
        }
        return (ordinal, body);
    }
}

/// Read frames until `target` ordinal, accumulating any SessionOutput bytes seen.
async fn recv_until(rd: &mut OwnedReadHalf, target: u16) -> (Vec<u8>, Vec<u8>) {
    let mut output = Vec::new();
    loop {
        let (ordinal, body) = recv(rd).await;
        if ordinal == ord::SESSION_OUTPUT {
            let r = wire::SessionOutputRef::read_as_root(&body).unwrap();
            if let Some(d) = r.data().ok().flatten() {
                output.extend_from_slice(d);
            }
        }
        if ordinal == target {
            return (body, output);
        }
    }
}

#[tokio::test]
async fn end_to_end_m1_lifecycle() {
    let dir = std::env::temp_dir().join(format!("asmux-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("asmux.sock");

    let listener = UnixListener::bind(&sock).unwrap();
    let registry = Arc::new(Registry::new(
        "test-instance".to_string(),
        123,
        MEMORY_LIMIT_DEFAULT_BYTES,
    ));
    let ctx = ServerCtx::new(registry, std::process::id() as i32, String::new());
    tokio::spawn(serve(listener, ctx));

    let stream = UnixStream::connect(&sock).await.unwrap();
    let (mut rd, mut wr) = stream.into_split();

    // --- hello ---
    let hello = wire::HelloRequest {
        rpc_id: 1,
        client_pid: std::process::id() as i32,
        client_name: Some("e2e".to_string()),
        protocol_min: 1,
        protocol_max: 1,
    };
    write_frame(&mut wr, frame::encode(ord::HELLO_REQUEST, &hello)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::HELLO_RESPONSE);
    let hr = wire::HelloResponseRef::read_as_root(&b).unwrap();
    assert_eq!(hr.protocol().unwrap(), 1);
    assert_eq!(hr.instance_id().unwrap(), Some("test-instance"));
    assert_eq!(hr.started_at_unix_ms().unwrap(), 123);

    // --- create (explicit id, `cat` echoes stdin) ---
    let create = wire::CreateRequest {
        rpc_id: 2,
        command: Some("cat".to_string()),
        args: None,
        cwd: None,
        env: None,
        cols: 80,
        rows: 24,
        metadata: Some(vec![wire::Kv {
            key: Some("label".to_string()),
            value: Some("shell".to_string()),
        }]),
        ring_capacity: 0,
        session_id: Some("s-fixed".to_string()),
    };
    write_frame(&mut wr, frame::encode(ord::CREATE_REQUEST, &create)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::CREATE_RESPONSE);
    let cr = wire::CreateResponseRef::read_as_root(&b).unwrap();
    let rec = cr.session().unwrap().unwrap();
    assert_eq!(rec.id().unwrap(), Some("s-fixed"));
    assert!(rec.alive().unwrap());
    assert!(rec.pid().unwrap() > 0);

    // --- idempotent re-create (same id + spec) returns the same session ---
    let create2 = wire::CreateRequest {
        rpc_id: 3,
        command: Some("cat".to_string()),
        args: None,
        cwd: None,
        env: None,
        cols: 80,
        rows: 24,
        metadata: None,
        ring_capacity: 0,
        session_id: Some("s-fixed".to_string()),
    };
    write_frame(&mut wr, frame::encode(ord::CREATE_REQUEST, &create2)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::CREATE_RESPONSE);
    let cr2 = wire::CreateResponseRef::read_as_root(&b).unwrap();
    assert_eq!(cr2.session().unwrap().unwrap().id().unwrap(), Some("s-fixed"));

    // --- create with same id but different command => SESSION_EXISTS ---
    let create3 = wire::CreateRequest {
        rpc_id: 4,
        command: Some("sh".to_string()),
        args: None,
        cwd: None,
        env: None,
        cols: 80,
        rows: 24,
        metadata: None,
        ring_capacity: 0,
        session_id: Some("s-fixed".to_string()),
    };
    write_frame(&mut wr, frame::encode(ord::CREATE_REQUEST, &create3)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::ERROR);
    let er = wire::ErrorRef::read_as_root(&b).unwrap();
    assert_eq!(er.code().unwrap(), frame::code::SESSION_EXISTS);

    // --- list shows our session ---
    let list = wire::ListRequest { rpc_id: 5 };
    write_frame(&mut wr, frame::encode(ord::LIST_REQUEST, &list)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::LIST_RESPONSE);
    let lr = wire::ListResponseRef::read_as_root(&b).unwrap();
    let sessions = lr.sessions().unwrap().unwrap();
    assert_eq!(sessions.len(), 1);

    // --- attach FromEarliest ---
    let attach = wire::AttachRequest {
        rpc_id: 6,
        session_id: Some("s-fixed".to_string()),
        mode: wire::AttachMode::FromEarliest,
        from_cursor: 0,
    };
    write_frame(&mut wr, frame::encode(ord::ATTACH_REQUEST, &attach)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::ATTACH_RESPONSE);
    let _ar = wire::AttachResponseRef::read_as_root(&b).unwrap();

    // --- input -> output echo ---
    let input = wire::SessionInput {
        session_id: Some("s-fixed".to_string()),
        data: Some(b"ping\n".to_vec()),
    };
    write_frame(&mut wr, frame::encode(ord::SESSION_INPUT, &input)).await;
    // cat (and the PTY) echo "ping"; collect output until we see it.
    let mut seen = Vec::new();
    loop {
        let (ordinal, body) = recv(&mut rd).await;
        if ordinal == ord::SESSION_OUTPUT {
            let r = wire::SessionOutputRef::read_as_root(&body).unwrap();
            if let Some(d) = r.data().ok().flatten() {
                seen.extend_from_slice(d);
            }
            if seen.windows(4).any(|w| w == b"ping") {
                break;
            }
        }
    }

    // --- resize ---
    let resize = wire::ResizeRequest {
        rpc_id: 7,
        session_id: Some("s-fixed".to_string()),
        cols: 120,
        rows: 40,
    };
    write_frame(&mut wr, frame::encode(ord::RESIZE_REQUEST, &resize)).await;
    let (o, _b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::RESIZE_RESPONSE);

    // --- updateMetadata ---
    let meta = wire::UpdateMetadataRequest {
        rpc_id: 8,
        session_id: Some("s-fixed".to_string()),
        patch: Some(vec![wire::Kv {
            key: Some("branch".to_string()),
            value: Some("main".to_string()),
        }]),
    };
    write_frame(&mut wr, frame::encode(ord::UPDATE_METADATA_REQUEST, &meta)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::UPDATE_METADATA_RESPONSE);
    let mr = wire::UpdateMetadataResponseRef::read_as_root(&b).unwrap();
    // resize is reflected in the record.
    assert_eq!(mr.session().unwrap().unwrap().cols().unwrap(), 120);

    // --- kill -> SessionExited ---
    let kill = wire::KillRequest {
        rpc_id: 9,
        session_id: Some("s-fixed".to_string()),
        signal: 0,
    };
    write_frame(&mut wr, frame::encode(ord::KILL_REQUEST, &kill)).await;
    let (o, _b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::KILL_RESPONSE);
    let (exit_body, _out) = recv_until(&mut rd, ord::SESSION_EXITED).await;
    let ex = wire::SessionExitedRef::read_as_root(&exit_body).unwrap();
    assert_eq!(ex.session_id().unwrap(), Some("s-fixed"));

    // --- purge the tombstone ---
    let purge = wire::PurgeRequest {
        rpc_id: 10,
        session_id: Some("s-fixed".to_string()),
    };
    write_frame(&mut wr, frame::encode(ord::PURGE_REQUEST, &purge)).await;
    let (o, _b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::PURGE_RESPONSE);

    // gone now.
    let list2 = wire::ListRequest { rpc_id: 11 };
    write_frame(&mut wr, frame::encode(ord::LIST_REQUEST, &list2)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::LIST_RESPONSE);
    let lr2 = wire::ListResponseRef::read_as_root(&b).unwrap();
    assert_eq!(lr2.sessions().unwrap().unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn hello_required_first() {
    let dir = std::env::temp_dir().join(format!("asmux-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("asmux.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let registry = Arc::new(Registry::new("i".to_string(), 0, MEMORY_LIMIT_DEFAULT_BYTES));
    let ctx = ServerCtx::new(registry, 0, String::new());
    tokio::spawn(serve(listener, ctx));

    let stream = UnixStream::connect(&sock).await.unwrap();
    let (mut rd, mut wr) = stream.into_split();

    // A non-hello first frame is a protocol error.
    let list = wire::ListRequest { rpc_id: 1 };
    write_frame(&mut wr, frame::encode(ord::LIST_REQUEST, &list)).await;
    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::ERROR);
    let er = wire::ErrorRef::read_as_root(&b).unwrap();
    assert_eq!(er.code().unwrap(), frame::code::PROTOCOL_MISMATCH);

    let _ = std::fs::remove_dir_all(&dir);
}
