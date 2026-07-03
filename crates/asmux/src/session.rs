//! A single live session: a PTY master + child + raw-byte ring.
//!
//! Two OS threads back each session:
//! - a **reader thread** that owns the blocking PTY read loop, appends every
//!   chunk to the ring, wakes any streamer, and on EOF reaps the child and
//!   records its exit status;
//! - a **writer thread** that drains a bounded input queue into the PTY with
//!   blocking writes, so a child that stops reading its PTY can never stall the
//!   connection reader (input overflows are dropped, never blocked).
//!
//! There is deliberately no `vt100` here — terminal interpretation is the
//! daemon's job (see `docs/asmux-protocol.md` → Never-crash invariants).

use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use portable_pty::{
    native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize,
};
use tokio::sync::Notify;

use crate::ring::{ReadOutcome, Ring};
use crate::wire;
use crate::INPUT_QUEUE_BYTES;

/// Largest chunk copied out of the ring per streamer read (keeps a single
/// `SessionOutput` frame well under the 16 MiB frame cap).
const STREAM_CHUNK: usize = 256 * 1024;
/// PTY read buffer size.
const READ_BUF: usize = 65536;

/// What a session needs to spawn a child.
pub struct SpawnSpec {
    pub session_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    pub ring_capacity: usize,
    pub metadata: Vec<(String, String)>,
    /// Immutable launch fingerprint used for `create` idempotency.
    pub fingerprint: u64,
    pub created_at_unix_ms: i64,
}

/// Terminal status of a session's child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running,
    /// Exited normally with `code` (when `signal == 0`) or was signalled
    /// (`signal != 0`, in which case `code` is -1).
    Exited { code: i32, signal: i32 },
}

impl Status {
    pub fn is_alive(&self) -> bool {
        matches!(self, Status::Running)
    }
}

/// Outcome of enqueuing input for the child.
#[derive(Debug, PartialEq, Eq)]
pub enum InputOutcome {
    Queued,
    /// Per-session input queue full — input dropped (`INPUT_OVERFLOW`).
    Overflow,
    /// Session is not alive; input is rejected (`SESSION_NOT_ALIVE`).
    NotAlive,
}

/// Why spawning a session failed.
#[derive(Debug)]
pub enum SpawnError {
    /// openpty / fork / exec failed (`SPAWN_FAILED`).
    Spawn(String),
}

/// The single current attacher of a session (single-attacher with takeover). A
/// new attach swaps this out and the server notifies the evicted one via its
/// `cancel` (stop streaming) and `ctrl_tx` (deliver `SessionDetached`).
#[derive(Clone)]
pub struct Attacher {
    pub conn_id: u64,
    pub cancel: Arc<Notify>,
    pub ctrl_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
}

pub struct Session {
    pub id: String,
    pub command: String,
    pub fingerprint: u64,
    pub created_at_unix_ms: i64,
    pub pid: i32,

    geom: Mutex<(u16, u16)>, // (cols, rows)
    ring: Mutex<Ring>,
    status: Mutex<Status>,
    metadata: Mutex<Vec<(String, String)>>,
    /// Woken on every ring append and on child exit, so a streamer can flush.
    data_signal: Arc<Notify>,

    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,

    input_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    input_queued: Arc<AtomicUsize>,

    /// The one connection currently streaming this session, if any.
    attacher: Mutex<Option<Attacher>>,
}

impl Session {
    /// Spawn the child and start the reader/writer threads. The returned
    /// `Arc<Session>` is what the registry stores and the server shares.
    pub fn spawn(spec: SpawnSpec) -> Result<Arc<Session>, SpawnError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SpawnError::Spawn(format!("openpty: {e}")))?;

        let mut cmd = CommandBuilder::new(&spec.command);
        for a in &spec.args {
            cmd.arg(a);
        }
        if !spec.cwd.is_empty() {
            cmd.cwd(&spec.cwd);
        }
        let mut have_term = false;
        for (k, v) in &spec.env {
            if k == "TERM" {
                have_term = true;
            }
            cmd.env(k, v);
        }
        if !have_term {
            cmd.env("TERM", "xterm-256color");
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| SpawnError::Spawn(format!("spawn `{}`: {e}", spec.command)))?;
        // Release the slave so the master read yields EOF when the child exits.
        drop(pair.slave);

        let pid = child.process_id().map(|p| p as i32).unwrap_or(-1);
        let killer = child.clone_killer();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| SpawnError::Spawn(format!("clone reader: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| SpawnError::Spawn(format!("take writer: {e}")))?;

        let data_signal = Arc::new(Notify::new());
        let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let input_queued = Arc::new(AtomicUsize::new(0));

        let session = Arc::new(Session {
            id: spec.session_id.clone(),
            command: spec.command.clone(),
            fingerprint: spec.fingerprint,
            created_at_unix_ms: spec.created_at_unix_ms,
            pid,
            geom: Mutex::new((spec.cols, spec.rows)),
            ring: Mutex::new(Ring::new(spec.ring_capacity)),
            status: Mutex::new(Status::Running),
            metadata: Mutex::new(spec.metadata),
            data_signal: data_signal.clone(),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            input_tx,
            input_queued: input_queued.clone(),
            attacher: Mutex::new(None),
        });

        // Reader thread.
        {
            let session = session.clone();
            let id = spec.session_id.clone();
            std::thread::Builder::new()
                .name(format!("asmux-rd-{}", short(&id)))
                .spawn(move || reader_loop(reader, session, child))
                .map_err(|e| SpawnError::Spawn(format!("reader thread: {e}")))?;
        }
        // Writer thread.
        {
            let id = spec.session_id.clone();
            std::thread::Builder::new()
                .name(format!("asmux-wr-{}", short(&id)))
                .spawn(move || writer_loop(writer, input_rx, input_queued))
                .map_err(|e| SpawnError::Spawn(format!("writer thread: {e}")))?;
        }

        Ok(session)
    }

    pub fn status(&self) -> Status {
        *self.status.lock()
    }

    pub fn is_alive(&self) -> bool {
        self.status.lock().is_alive()
    }

    pub fn head(&self) -> u64 {
        self.ring.lock().head()
    }

    pub fn tail(&self) -> u64 {
        self.ring.lock().tail()
    }

    pub fn ring_capacity(&self) -> u64 {
        self.ring.lock().capacity() as u64
    }

    /// Install `new` as the sole attacher, returning the one it replaced (if any)
    /// so the server can evict it (single-attacher with takeover).
    pub fn attach(&self, new: Attacher) -> Option<Attacher> {
        self.attacher.lock().replace(new)
    }

    /// Clear the attacher iff it is still `conn_id` (a later takeover by another
    /// connection must not be clobbered by this one's teardown).
    pub fn detach(&self, conn_id: u64) {
        let mut a = self.attacher.lock();
        if a.as_ref().map(|x| x.conn_id) == Some(conn_id) {
            *a = None;
        }
    }

    pub fn is_attached_by(&self, conn_id: u64) -> bool {
        self.attacher.lock().as_ref().map(|x| x.conn_id) == Some(conn_id)
    }

    /// Await the next ring append or exit signal. The caller creates this future
    /// *before* reading `head`, so a wake that races the read is not lost
    /// (`Notify` stores a single permit).
    pub fn notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.data_signal.notified()
    }

    /// Copy up to `max` bytes at cursor `from` (0 => a default chunk).
    pub fn read_at(&self, from: u64, max: usize) -> ReadOutcome {
        let cap = if max == 0 { STREAM_CHUNK } else { max };
        self.ring.lock().read_at(from, cap)
    }

    pub fn geometry(&self) -> (u16, u16) {
        *self.geom.lock()
    }

    /// Enqueue input for the child. Never blocks: overflow drops the input.
    pub fn send_input(&self, data: &[u8]) -> InputOutcome {
        if !self.is_alive() {
            return InputOutcome::NotAlive;
        }
        let len = data.len();
        let queued = self.input_queued.load(Ordering::Acquire);
        if queued.saturating_add(len) > INPUT_QUEUE_BYTES {
            return InputOutcome::Overflow;
        }
        self.input_queued.fetch_add(len, Ordering::AcqRel);
        if self.input_tx.send(data.to_vec()).is_err() {
            // Writer thread gone (child dead): undo the reservation.
            self.input_queued.fetch_sub(len, Ordering::AcqRel);
            return InputOutcome::NotAlive;
        }
        InputOutcome::Queued
    }

    /// Resize the PTY. Rejected on a dead session.
    pub fn resize(&self, cols: u16, rows: u16) -> InputOutcome {
        if !self.is_alive() {
            return InputOutcome::NotAlive;
        }
        {
            let master = self.master.lock();
            if master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .is_err()
            {
                return InputOutcome::NotAlive;
            }
        }
        *self.geom.lock() = (cols, rows);
        InputOutcome::Queued
    }

    /// Signal the child. `signal == 0` => platform default terminate. Idempotent
    /// on an already-dead session (the reaper handles the actual status).
    pub fn kill(&self, signal: i32) {
        if !self.is_alive() {
            return;
        }
        if signal == 0 {
            let _ = self.killer.lock().kill();
            return;
        }
        #[cfg(unix)]
        {
            if let Ok(sig) = nix::sys::signal::Signal::try_from(signal) {
                let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(self.pid), sig);
                return;
            }
        }
        // Unknown signal or non-unix: fall back to the default terminate.
        let _ = self.killer.lock().kill();
    }

    pub fn metadata(&self) -> Vec<(String, String)> {
        self.metadata.lock().clone()
    }

    /// Apply a metadata patch: `Some(v)` sets (including `""`), `None` deletes.
    pub fn patch_metadata(&self, patch: &[(String, Option<String>)]) {
        let mut md = self.metadata.lock();
        for (k, v) in patch {
            md.retain(|(ek, _)| ek != k);
            if let Some(val) = v {
                md.push((k.clone(), val.clone()));
            }
        }
    }

    /// Build the frozen `SessionRecord` snapshot for `list`/`create` responses.
    pub fn record(&self) -> wire::SessionRecord {
        let (cols, rows) = self.geometry();
        let (alive, exit_code, exit_signal) = match self.status() {
            Status::Running => (true, 0, 0),
            Status::Exited { code, signal } => (false, code, signal),
        };
        let (head, tail, cap) = {
            let ring = self.ring.lock();
            (ring.head(), ring.tail(), ring.capacity() as u64)
        };
        let metadata = self
            .metadata()
            .into_iter()
            .map(|(k, v)| wire::Kv {
                key: Some(k),
                value: Some(v),
            })
            .collect();
        wire::SessionRecord {
            id: Some(self.id.clone()),
            alive,
            pid: self.pid,
            exit_code,
            exit_signal,
            cols,
            rows,
            head_cursor: head,
            tail_cursor: tail,
            ring_capacity: cap,
            created_at_unix_ms: self.created_at_unix_ms,
            command: Some(self.command.clone()),
            metadata: Some(metadata),
        }
    }
}

/// Reader thread: blocking PTY reads → ring, wake streamer, then reap.
fn reader_loop(mut reader: Box<dyn Read + Send>, session: Arc<Session>, mut child: Box<dyn Child + Send + Sync>) {
    let mut buf = [0u8; READ_BUF];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(chunk) = buf.get(..n) {
                    // A failed ring alloc (ALLOC_FAILED territory) must not crash
                    // the holder: drop the chunk, keep the session readable.
                    let _ = session.ring.lock().push(chunk);
                    session.data_signal.notify_one();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    let status = match child.wait() {
        Ok(es) => {
            if es.success() {
                Status::Exited {
                    code: 0,
                    signal: 0,
                }
            } else {
                // portable_pty does not surface the signal separately; record the
                // raw code. Signal decomposition is a daemon-side refinement.
                Status::Exited {
                    code: es.exit_code() as i32,
                    signal: 0,
                }
            }
        }
        Err(_) => Status::Exited {
            code: -1,
            signal: 0,
        },
    };
    *session.status.lock() = status;
    // Wake the streamer so it can flush the tail and emit SessionExited.
    session.data_signal.notify_one();
}

/// Writer thread: drain the input queue into the PTY with blocking writes.
fn writer_loop(
    mut writer: Box<dyn Write + Send>,
    mut input_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    input_queued: Arc<AtomicUsize>,
) {
    while let Some(data) = input_rx.blocking_recv() {
        let len = data.len();
        let _ = writer.write_all(&data);
        let _ = writer.flush();
        input_queued.fetch_sub(len, Ordering::AcqRel);
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
