use std::io::{Read, Write};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::{broadcast, watch};

use super::{
    BackendSession, BackendSpawnSpec, BackendStatus, HistoryRing, SessionBackend, Snapshot,
    HISTORY_RING_BYTES,
};
use crate::db::{EventMsg, EventSink};
use crate::util::now_millis;

const BROADCAST_CAP: usize = 2048;
const SCROLLBACK: usize = 2000;
const READ_BUF: usize = 8192;

/// The MVP built-in backend: one native PTY per live session, driving a
/// headless `vt100` terminal emulator whose screen is the resume source.
///
/// The architecture calls for an out-of-process sidecar per session so PTYs
/// survive daemon restart. This in-process implementation satisfies the
/// same `SessionBackend` contract; extracting it into a sidecar is a later
/// iteration behind this identical trait boundary.
pub struct NativePtyBackend {
    events: EventSink,
}

impl NativePtyBackend {
    pub fn new(events: EventSink) -> Self {
        Self { events }
    }
}

impl SessionBackend for NativePtyBackend {
    fn id(&self) -> &'static str {
        "native-pty"
    }

    fn create(&self, spec: BackendSpawnSpec) -> Result<Arc<dyn BackendSession>> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("openpty failed")?;

        let mut cmd = CommandBuilder::new(&spec.command);
        for a in &spec.args {
            cmd.arg(a);
        }
        cmd.cwd(&spec.cwd);
        // Don't let an enclosing Claude/Codex session leak its identity into
        // the agent (it would run as a nested child session; see asmux).
        asmux::session::scrub_inherited_agent_env(&mut cmd);
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
            .with_context(|| format!("spawning `{}`", spec.command))?;
        // Release the slave so the master read yields EOF when the child exits.
        drop(pair.slave);

        let killer = child.clone_killer();
        let reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;

        let (tx, _rx) = broadcast::channel::<Arc<[u8]>>(BROADCAST_CAP);
        let (status_tx, status_rx) = watch::channel(BackendStatus::Running);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            spec.rows, spec.cols, SCROLLBACK,
        )));
        let history = Arc::new(Mutex::new(HistoryRing::new(HISTORY_RING_BYTES)));
        let seq = Arc::new(AtomicU64::new(0));

        let session = Arc::new(NativeSession {
            parser: parser.clone(),
            history: history.clone(),
            tx: tx.clone(),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            status_rx,
            seq: seq.clone(),
        });

        // Reader thread: owns the blocking PTY read loop, feeds the emulator,
        // persists events, and broadcasts to live subscribers.
        let events = self.events.clone();
        let session_id = spec.session_id.clone();
        std::thread::Builder::new()
            .name(format!("asm-pty-{}", short(&session_id)))
            .spawn(move || {
                reader_loop(reader, parser, history, tx, events, session_id, seq, status_tx, child);
            })
            .context("spawning pty reader thread")?;

        Ok(session)
    }
}

struct NativeSession {
    parser: Arc<Mutex<vt100::Parser>>,
    history: Arc<Mutex<HistoryRing>>,
    tx: broadcast::Sender<Arc<[u8]>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    status_rx: watch::Receiver<BackendStatus>,
    seq: Arc<AtomicU64>,
}

impl BackendSession for NativeSession {
    fn attach(&self) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>) {
        // The reader processes+broadcasts (and pushes the ring) under the locks
        // the helper holds, so the receiver starts exactly where the snapshot
        // ends.
        super::attach_with_history(&self.parser, &self.history, &self.tx, &self.seq)
    }

    fn snapshot(&self) -> Snapshot {
        super::snapshot_screen(&self.parser.lock(), &self.seq)
    }

    fn screen_text(&self) -> String {
        self.parser.lock().screen().contents()
    }

    fn send_input(&self, data: &[u8]) -> Result<()> {
        let mut w = self.writer.lock();
        w.write_all(data).context("writing pty input")?;
        w.flush().context("flushing pty input")?;
        Ok(())
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        {
            let master = self.master.lock();
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .context("resizing pty")?;
        }
        self.parser.lock().set_size(rows, cols);
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        self.killer.lock().kill().context("killing child process")?;
        Ok(())
    }

    fn watch_status(&self) -> watch::Receiver<BackendStatus> {
        self.status_rx.clone()
    }

    fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

#[allow(clippy::too_many_arguments)]
fn reader_loop(
    mut reader: Box<dyn Read + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    history: Arc<Mutex<HistoryRing>>,
    tx: broadcast::Sender<Arc<[u8]>>,
    events: EventSink,
    session_id: String,
    seq: Arc<AtomicU64>,
    status_tx: watch::Sender<BackendStatus>,
    mut child: Box<dyn Child + Send + Sync>,
) {
    let mut buf = [0u8; READ_BUF];
    let mut parser_ok = true;
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                let s = seq.fetch_add(1, Ordering::SeqCst) + 1;
                let arc: Arc<[u8]> = Arc::from(chunk.to_vec().into_boxed_slice());

                // Process the emulator and broadcast under the same lock so
                // attach()'s snapshot+subscribe stays perfectly ordered.
                {
                    let mut p = parser.lock();
                    if parser_ok {
                        let res = std::panic::catch_unwind(AssertUnwindSafe(|| p.process(chunk)));
                        if res.is_err() {
                            // Isolate a parser panic to this one session.
                            parser_ok = false;
                            tracing::error!(session = %session_id, "terminal parser panicked; snapshots disabled for this session");
                        }
                    }
                    events.send(EventMsg {
                        session_id: session_id.clone(),
                        seq: s,
                        ts_ms: now_millis(),
                        stream: 0,
                        bytes: chunk.to_vec(),
                    });
                    // Push the ring and broadcast under the ring lock so a
                    // normal-buffer attach (which reads the ring + subscribes
                    // under that lock) sees a single consistent stream.
                    {
                        let mut h = history.lock();
                        h.push(arc.clone());
                        let _ = tx.send(arc);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::warn!(session = %session_id, "pty read error: {e}");
                break;
            }
        }
    }

    let status = match child.wait() {
        Ok(es) => {
            if es.success() {
                BackendStatus::Exited(0)
            } else {
                BackendStatus::Exited(es.exit_code() as i32)
            }
        }
        Err(e) => BackendStatus::Failed(format!("wait failed: {e}")),
    };
    tracing::info!(session = %session_id, ?status, "pty session ended");
    let _ = status_tx.send(status);
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
