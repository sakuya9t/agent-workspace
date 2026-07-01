use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, watch};

pub mod native;

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
    fn status(&self) -> BackendStatus;
    fn watch_status(&self) -> watch::Receiver<BackendStatus>;
    fn last_seq(&self) -> u64;
}
