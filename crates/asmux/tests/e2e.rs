//! End-to-end M1 verification: spin up the real asmux server on a temp socket
//! and drive it through the full session lifecycle as the client would —
//! hello → create → idempotent re-create → list → attach → input/output →
//! resize → metadata → kill → SessionExited → purge.
//!
//! Integration tests are a separate crate, so the holder's `#![deny]` never-crash
//! lints don't apply here; `unwrap`/`panic` are fine in test scaffolding.

use std::sync::Arc;
use std::time::Duration;

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

async fn start_test_server(name: &str) -> (std::path::PathBuf, std::path::PathBuf, Arc<Registry>) {
    let dir =
        std::env::temp_dir().join(format!("asmux-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("asmux.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let registry = Arc::new(Registry::new(
        format!("{name}-instance"),
        123,
        MEMORY_LIMIT_DEFAULT_BYTES,
    ));
    let ctx = ServerCtx::new(registry.clone(), std::process::id() as i32, String::new());
    tokio::spawn(serve(listener, ctx));
    (dir, sock, registry)
}

async fn connect_and_hello(sock: &std::path::Path) -> (OwnedReadHalf, OwnedWriteHalf) {
    let stream = UnixStream::connect(sock).await.unwrap();
    let (mut rd, mut wr) = stream.into_split();
    let hello = wire::HelloRequest {
        rpc_id: 1,
        client_pid: std::process::id() as i32,
        client_name: Some("e2e".to_string()),
        protocol_min: 1,
        protocol_max: 1,
    };
    write_frame(&mut wr, frame::encode(ord::HELLO_REQUEST, &hello)).await;
    let (ordinal, _) = recv_resp(&mut rd).await;
    assert_eq!(ordinal, ord::HELLO_RESPONSE);
    (rd, wr)
}

async fn create_session(
    rd: &mut OwnedReadHalf,
    wr: &mut OwnedWriteHalf,
    rpc_id: u64,
    session_id: &str,
    command: &str,
    args: Option<Vec<String>>,
    ring_capacity: u64,
) {
    let create = wire::CreateRequest {
        rpc_id,
        command: Some(command.to_string()),
        args,
        cwd: None,
        env: None,
        cols: 80,
        rows: 24,
        metadata: None,
        ring_capacity,
        session_id: Some(session_id.to_string()),
    };
    write_frame(wr, frame::encode(ord::CREATE_REQUEST, &create)).await;
    let (ordinal, body) = recv_resp(rd).await;
    assert_eq!(
        ordinal,
        ord::CREATE_RESPONSE,
        "create failed: {:?}",
        wire::ErrorRef::read_as_root(&body).ok()
    );
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

/// Dropped output must reach the attacher as `ALLOC_FAILED`.
///
/// The reader thread discards a chunk when the ring's fallible reserve fails —
/// the never-crash rule — and a failed push does not advance `head`, so the loss
/// leaves no cursor gap for anyone to notice. `ALLOC_FAILED` used to be a
/// constant no code path emitted, meaning output vanished under exactly the
/// memory pressure the design exists to survive, with no log and no signal.
///
/// A real allocation failure can't be provoked here, so the drop is recorded
/// through the same call the reader thread makes; what is under test is the
/// reporting path from that record to the attacher's socket.
#[tokio::test]
async fn dropped_output_is_reported_to_the_attacher() {
    let dir = std::env::temp_dir().join(format!("asmux-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("asmux.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let registry = Arc::new(Registry::new(
        "drop-instance".to_string(),
        0,
        MEMORY_LIMIT_DEFAULT_BYTES,
    ));
    let ctx = ServerCtx::new(registry.clone(), std::process::id() as i32, String::new());
    tokio::spawn(serve(listener, ctx));

    let stream = UnixStream::connect(&sock).await.unwrap();
    let (mut rd, mut wr) = stream.into_split();

    let hello = wire::HelloRequest {
        rpc_id: 1,
        client_pid: std::process::id() as i32,
        client_name: Some("e2e".to_string()),
        protocol_min: 1,
        protocol_max: 1,
    };
    write_frame(&mut wr, frame::encode(ord::HELLO_REQUEST, &hello)).await;
    let (o, _) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::HELLO_RESPONSE);

    let create = wire::CreateRequest {
        rpc_id: 2,
        command: Some("cat".to_string()),
        args: None,
        cwd: None,
        env: None,
        cols: 80,
        rows: 24,
        metadata: None,
        ring_capacity: 0,
        session_id: Some("s-drop".to_string()),
    };
    write_frame(&mut wr, frame::encode(ord::CREATE_REQUEST, &create)).await;
    let (o, _) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::CREATE_RESPONSE);

    let attach = wire::AttachRequest {
        rpc_id: 3,
        session_id: Some("s-drop".to_string()),
        mode: wire::AttachMode::FromEarliest,
        from_cursor: 0,
    };
    write_frame(&mut wr, frame::encode(ord::ATTACH_REQUEST, &attach)).await;
    let (o, _) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::ATTACH_RESPONSE);

    // What the reader thread does when `Ring::push` returns AllocFailed.
    let session = registry.get("s-drop").expect("session must exist");
    session.note_output_dropped(4096);

    let (o, b) = recv_resp(&mut rd).await;
    assert_eq!(o, ord::ERROR, "the attacher must be told output was lost");
    let er = wire::ErrorRef::read_as_root(&b).unwrap();
    assert_eq!(er.code().unwrap(), frame::code::ALLOC_FAILED);
    assert_eq!(er.session_id().unwrap(), Some("s-drop"));
    assert_eq!(er.rpc_id().unwrap(), 0, "unsolicited, not an RPC reply");

    session.kill(9);
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

#[tokio::test]
async fn read_buffer_reports_partial_gap_and_invalid_cursors() {
    let (dir, sock, registry) = start_test_server("read-buffer").await;
    let (mut rd, mut wr) = connect_and_hello(&sock).await;
    create_session(
        &mut rd,
        &mut wr,
        2,
        "s-read",
        "sh",
        Some(vec![
            "-c".into(),
            "head -c 32768 /dev/zero | tr '\\0' x; exec sleep 30".into(),
        ]),
        asmux::RING_MIN_BYTES,
    )
    .await;

    let session = registry.get("s-read").unwrap();
    let (tail, head) = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let tail = session.tail();
            let head = session.head();
            if tail > 0 && head >= 32768 {
                break (tail, head);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("producer should wrap the minimum ring");

    let partial = wire::ReadBufferRequest {
        rpc_id: 3,
        session_id: Some("s-read".into()),
        from_cursor: tail,
        max_bytes: 32,
    };
    write_frame(
        &mut wr,
        frame::encode(ord::READ_BUFFER_REQUEST, &partial),
    )
    .await;
    let (ordinal, body) = recv_resp(&mut rd).await;
    assert_eq!(ordinal, ord::READ_BUFFER_RESPONSE);
    let response = wire::ReadBufferResponseRef::read_as_root(&body).unwrap();
    assert_eq!(response.rpc_id().unwrap(), 3);
    assert_eq!(response.from_cursor().unwrap(), tail);
    assert_eq!(response.head_cursor().unwrap(), head);
    assert_eq!(response.data().unwrap().unwrap().len(), 32);

    let gap = wire::ReadBufferRequest {
        rpc_id: 4,
        session_id: Some("s-read".into()),
        from_cursor: 0,
        max_bytes: 0,
    };
    write_frame(&mut wr, frame::encode(ord::READ_BUFFER_REQUEST, &gap)).await;
    let (ordinal, body) = recv_resp(&mut rd).await;
    assert_eq!(ordinal, ord::ERROR);
    let error = wire::ErrorRef::read_as_root(&body).unwrap();
    assert_eq!(error.rpc_id().unwrap(), 4);
    assert_eq!(error.code().unwrap(), frame::code::BUFFER_GAP);
    assert_eq!(error.earliest_cursor().unwrap(), tail);

    let invalid = wire::ReadBufferRequest {
        rpc_id: 5,
        session_id: Some("s-read".into()),
        from_cursor: head + 1,
        max_bytes: 0,
    };
    write_frame(
        &mut wr,
        frame::encode(ord::READ_BUFFER_REQUEST, &invalid),
    )
    .await;
    let (ordinal, body) = recv_resp(&mut rd).await;
    assert_eq!(ordinal, ord::ERROR);
    let error = wire::ErrorRef::read_as_root(&body).unwrap();
    assert_eq!(error.rpc_id().unwrap(), 5);
    assert_eq!(error.code().unwrap(), frame::code::INVALID_ARGUMENT);

    session.kill(9);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn detach_is_owner_checked_and_cross_connection_attach_takes_over() {
    let (dir, sock, registry) = start_test_server("takeover").await;
    let (mut first_rd, mut first_wr) = connect_and_hello(&sock).await;
    let (mut second_rd, mut second_wr) = connect_and_hello(&sock).await;
    create_session(
        &mut first_rd,
        &mut first_wr,
        2,
        "s-takeover",
        "cat",
        None,
        0,
    )
    .await;

    let first_attach = wire::AttachRequest {
        rpc_id: 3,
        session_id: Some("s-takeover".into()),
        mode: wire::AttachMode::FromEarliest,
        from_cursor: 0,
    };
    write_frame(
        &mut first_wr,
        frame::encode(ord::ATTACH_REQUEST, &first_attach),
    )
    .await;
    assert_eq!(recv_resp(&mut first_rd).await.0, ord::ATTACH_RESPONSE);

    let second_attach = wire::AttachRequest {
        rpc_id: 2,
        session_id: Some("s-takeover".into()),
        mode: wire::AttachMode::FromEarliest,
        from_cursor: 0,
    };
    write_frame(
        &mut second_wr,
        frame::encode(ord::ATTACH_REQUEST, &second_attach),
    )
    .await;
    assert_eq!(recv_resp(&mut second_rd).await.0, ord::ATTACH_RESPONSE);

    let (ordinal, body) = recv_resp(&mut first_rd).await;
    assert_eq!(ordinal, ord::SESSION_DETACHED);
    let detached = wire::SessionDetachedRef::read_as_root(&body).unwrap();
    assert_eq!(
        detached.reason().unwrap(),
        wire::DetachReason::Superseded
    );

    // The superseded connection no longer owns this session and cannot detach
    // the replacement. The replacement can detach itself normally.
    let stale_detach = wire::DetachRequest {
        rpc_id: 4,
        session_id: Some("s-takeover".into()),
    };
    write_frame(
        &mut first_wr,
        frame::encode(ord::DETACH_REQUEST, &stale_detach),
    )
    .await;
    let (ordinal, body) = recv_resp(&mut first_rd).await;
    assert_eq!(ordinal, ord::ERROR);
    assert_eq!(
        wire::ErrorRef::read_as_root(&body).unwrap().code().unwrap(),
        frame::code::NOT_ATTACHED
    );

    let owner_detach = wire::DetachRequest {
        rpc_id: 3,
        session_id: Some("s-takeover".into()),
    };
    write_frame(
        &mut second_wr,
        frame::encode(ord::DETACH_REQUEST, &owner_detach),
    )
    .await;
    assert_eq!(recv_resp(&mut second_rd).await.0, ord::DETACH_RESPONSE);

    registry.get("s-takeover").unwrap().kill(9);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn slow_stream_is_evicted_with_backpressure_reason() {
    let (dir, sock, registry) = start_test_server("backpressure").await;
    let (mut rd, mut wr) = connect_and_hello(&sock).await;
    create_session(
        &mut rd,
        &mut wr,
        2,
        "s-slow",
        "sh",
        None,
        asmux::RING_MIN_BYTES,
    )
    .await;

    let attach = wire::AttachRequest {
        rpc_id: 3,
        session_id: Some("s-slow".into()),
        mode: wire::AttachMode::FromEarliest,
        from_cursor: 0,
    };
    write_frame(&mut wr, frame::encode(ord::ATTACH_REQUEST, &attach)).await;
    assert_eq!(recv_resp(&mut rd).await.0, ord::ATTACH_RESPONSE);

    // Replace the shell with a continuous writer, then deliberately stop
    // reading its socket until the bounded data channel fills. The PTY reader
    // keeps advancing the 16 KiB ring in the meantime, forcing this stream's
    // cursor behind the tail. Only this attachment is evicted and told to
    // resync. `exec` keeps the producer on the session pid so cleanup kills it.
    let input = wire::SessionInput {
        session_id: Some("s-slow".into()),
        data: Some(b"exec yes z\n".to_vec()),
    };
    write_frame(&mut wr, frame::encode(ord::SESSION_INPUT, &input)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let body = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (ordinal, body) = recv(&mut rd).await;
            if ordinal == ord::SESSION_DETACHED {
                break body;
            }
        }
    })
    .await
    .expect("slow stream should be evicted once its cursor falls behind the ring");
    let detached = wire::SessionDetachedRef::read_as_root(&body).unwrap();
    assert_eq!(
        detached.reason().unwrap(),
        wire::DetachReason::Backpressure
    );
    assert_eq!(detached.session_id().unwrap(), Some("s-slow"));

    registry.get("s-slow").unwrap().kill(9);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn malformed_rpc_bodies_return_errors_and_keep_connection_usable() {
    let (dir, sock, _registry) = start_test_server("malformed").await;
    let (mut rd, mut wr) = connect_and_hello(&sock).await;

    let request_ordinals = [
        ord::CREATE_REQUEST,
        ord::KILL_REQUEST,
        ord::PURGE_REQUEST,
        ord::LIST_REQUEST,
        ord::UPDATE_METADATA_REQUEST,
        ord::RESIZE_REQUEST,
        ord::READ_BUFFER_REQUEST,
        ord::ATTACH_REQUEST,
        ord::DETACH_REQUEST,
        ord::SESSION_INPUT,
    ];
    for ordinal in request_ordinals {
        write_frame(&mut wr, frame::frame_body(ordinal, &[0])).await;
        let (response_ordinal, body) = recv_resp(&mut rd).await;
        assert_eq!(
            response_ordinal,
            ord::ERROR,
            "ordinal {ordinal} was silently dropped"
        );
        let error = wire::ErrorRef::read_as_root(&body).unwrap();
        assert_eq!(error.rpc_id().unwrap(), 0);
        assert_eq!(error.code().unwrap(), frame::code::INVALID_ARGUMENT);
    }

    // A malformed request is scoped to that request; it does not poison the
    // connection or leave the next valid RPC waiting behind a silent drop.
    let list = wire::ListRequest { rpc_id: 99 };
    write_frame(&mut wr, frame::encode(ord::LIST_REQUEST, &list)).await;
    let (ordinal, body) = recv_resp(&mut rd).await;
    assert_eq!(ordinal, ord::LIST_RESPONSE);
    assert_eq!(
        wire::ListResponseRef::read_as_root(&body)
            .unwrap()
            .rpc_id()
            .unwrap(),
        99
    );

    let _ = std::fs::remove_dir_all(dir);
}
