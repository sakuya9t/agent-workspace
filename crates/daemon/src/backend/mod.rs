use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, watch};

pub mod native;
pub mod sidecar;
pub mod asmux_client;

/// One session the holder still knows about, from `holder_list()`. Used at
/// startup to decide adopt (alive) vs reconcile-from-exit (dead) vs
/// reconcile-indeterminate (absent — the holder itself was gone).
#[derive(Debug, Clone)]
pub struct HolderEntry {
    pub id: String,
    pub alive: bool,
    pub exit_code: i32,
    pub exit_signal: i32,
}

/// Everything a backend needs to spawn one live session.
#[derive(Debug, Clone)]
pub struct BackendSpawnSpec {
    pub session_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub rows: u16,
    pub cols: u16,
}

/// Live backend status, mirrored into the session record by the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendStatus {
    Running,
    Exited(i32),
    Failed(String),
}

impl BackendStatus {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, BackendStatus::Running)
    }
}

/// A server-side terminal snapshot: an ANSI repaint stream plus geometry.
/// Written through the same xterm.js path as live output on the client.
///
/// `rows`/`cols`/`last_seq` are part of the snapshot resume contract (they will
/// feed persisted-snapshot history and the client resume point); only
/// `repaint` is consumed today.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub rows: u16,
    pub cols: u16,
    pub repaint: Arc<[u8]>,
    pub last_seq: u64,
}

/// Factory for live sessions. The native backend is registered under the
/// plugin registry; a mock backend implements the same trait in tests.
pub trait SessionBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn create(&self, spec: BackendSpawnSpec) -> Result<Arc<dyn BackendSession>>;

    /// Whether live sessions survive a daemon shutdown. `true` for an
    /// out-of-process holder (asmux): on shutdown the daemon detaches and leaves
    /// the children running to be re-adopted, rather than killing them. `false`
    /// for the in-process native backend, whose PTYs must be reaped.
    fn keep_sessions_on_shutdown(&self) -> bool {
        false
    }

    /// Sessions the out-of-process holder still knows about (empty for backends
    /// that don't outlive the daemon).
    fn holder_list(&self) -> Result<Vec<HolderEntry>> {
        Ok(Vec::new())
    }

    /// Re-adopt a session that is still alive in the holder after a daemon
    /// restart: seed a fresh daemon-side emulator from cold history and re-attach
    /// the holder ring. Returns `None` if this backend cannot adopt (native) or
    /// the session is not recoverable.
    fn adopt(&self, _session_id: &str, _rows: u16, _cols: u16) -> Result<Option<Arc<dyn BackendSession>>> {
        Ok(None)
    }
}

/// A single live session owned by a backend.
pub trait BackendSession: Send + Sync {
    /// Atomically capture the current emulator snapshot and subscribe to the
    /// live output stream. Ordering between snapshot and stream is guaranteed
    /// so a fresh client never sees a gap or a duplicated byte range.
    fn attach(&self) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>);

    /// Current emulator snapshot without subscribing.
    fn snapshot(&self) -> Snapshot;

    fn send_input(&self, data: &[u8]) -> Result<()>;
    fn resize(&self, rows: u16, cols: u16) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn watch_status(&self) -> watch::Receiver<BackendStatus>;
    fn last_seq(&self) -> u64;
}
