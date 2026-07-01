use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::backend::{BackendSession, BackendSpawnSpec, BackendStatus, SessionBackend};
use crate::db::Db;
use crate::domain::{AttentionState, Session, SessionStatus, SessionSummary};
use crate::plugins::{AgentContext, PluginRegistry};
use crate::util::now_millis;

/// Request to start a new session.
#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub agent_plugin_id: String,
    pub cwd: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub rows: u16,
    pub cols: u16,
    pub workspace_id: Option<String>,
    /// Explicit approval, required for the `custom_command` plugin.
    pub approve_custom: bool,
}

/// Owns session lifecycle: plugin resolution, backend spawn, persistence, and
/// the per-session monitor task that tracks exit, summaries, and attention.
pub struct SessionManager {
    pub db: Db,
    pub registry: Arc<PluginRegistry>,
    backend: Arc<dyn SessionBackend>,
    live: Mutex<HashMap<String, Arc<dyn BackendSession>>>,
}

impl SessionManager {
    pub fn new(db: Db, registry: Arc<PluginRegistry>, backend: Arc<dyn SessionBackend>) -> Self {
        Self {
            db,
            registry,
            backend,
            live: Mutex::new(HashMap::new()),
        }
    }

    pub fn backend_id(&self) -> &'static str {
        self.backend.id()
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        self.db.list_sessions()
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        self.db.get_session(id)
    }

    pub fn get_summary(&self, id: &str) -> Result<Option<SessionSummary>> {
        self.db.get_summary(id)
    }

    pub fn live_handle(&self, id: &str) -> Option<Arc<dyn BackendSession>> {
        self.live.lock().get(id).cloned()
    }

    pub fn live_count(&self) -> usize {
        self.live.lock().len()
    }

    pub fn create_session(self: &Arc<Self>, req: CreateSessionRequest) -> Result<Session> {
        let plugin = self
            .registry
            .get(&req.agent_plugin_id)
            .ok_or_else(|| anyhow!("unknown agent plugin `{}`", req.agent_plugin_id))?;

        let ctx = AgentContext {
            cwd: req.cwd.clone(),
            command: req.command.clone(),
            extra_args: req.args.clone(),
            extra_env: req.env.clone(),
        };
        let launch = plugin.build_launch(&ctx)?;

        if launch.requires_approval && !req.approve_custom {
            bail!("launch requires explicit approval (custom command)");
        }

        let cwd_path = std::path::Path::new(&req.cwd);
        if !cwd_path.is_dir() {
            bail!("working directory does not exist: {}", req.cwd);
        }

        let now = now_millis();
        let id = Uuid::new_v4().to_string();
        let session = Session {
            id: id.clone(),
            agent_plugin_id: plugin.id().to_string(),
            command: launch.command.clone(),
            args: launch.args.clone(),
            env: launch.env.clone(),
            working_directory: req.cwd.clone(),
            workspace_id: req.workspace_id.clone(),
            status: SessionStatus::Starting,
            rows: req.rows,
            cols: req.cols,
            last_event_seq: 0,
            exit_code: None,
            attention_state: AttentionState::None,
            attention_reason: None,
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        };
        self.db.insert_session(&session)?;

        let spec = BackendSpawnSpec {
            session_id: id.clone(),
            command: launch.command,
            args: launch.args,
            env: launch.env,
            cwd: req.cwd,
            rows: req.rows,
            cols: req.cols,
        };

        let handle = match self.backend.create(spec) {
            Ok(h) => h,
            Err(e) => {
                let now = now_millis();
                let _ = self.db.update_status(&id, SessionStatus::Failed, None, now);
                let _ = self.db.set_attention(
                    &id,
                    AttentionState::Failed,
                    Some("backend spawn failed"),
                    now,
                );
                return Err(e.context("backend failed to create session"));
            }
        };

        self.live.lock().insert(id.clone(), handle.clone());
        self.db
            .update_status(&id, SessionStatus::Running, None, now_millis())?;

        self.clone().spawn_monitor(id.clone(), handle, session.created_at);

        self.db
            .get_session(&id)?
            .ok_or_else(|| anyhow!("session vanished after creation"))
    }

    pub fn stop_session(&self, id: &str) -> Result<Session> {
        let handle = self.live_handle(id);
        match handle {
            Some(h) => {
                // Record intent first so the monitor keeps `stopped` on exit.
                self.db
                    .update_status(id, SessionStatus::Stopped, None, now_millis())?;
                h.stop()?;
            }
            None => {
                let s = self
                    .db
                    .get_session(id)?
                    .ok_or_else(|| anyhow!("no such session"))?;
                if !s.status.is_terminal() {
                    // Not live and not terminal: reconcile to stopped.
                    self.db
                        .update_status(id, SessionStatus::Stopped, None, now_millis())?;
                }
            }
        }
        self.db
            .get_session(id)?
            .ok_or_else(|| anyhow!("session vanished"))
    }

    pub fn archive_session(&self, id: &str) -> Result<Session> {
        let s = self
            .db
            .get_session(id)?
            .ok_or_else(|| anyhow!("no such session"))?;
        if !s.status.is_terminal() {
            bail!("cannot archive a live session; stop it first");
        }
        self.db
            .update_status(id, SessionStatus::Archived, s.exit_code, now_millis())?;
        self.db
            .get_session(id)?
            .ok_or_else(|| anyhow!("session vanished"))
    }

    pub fn resize_session(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        if let Some(h) = self.live_handle(id) {
            h.resize(rows, cols)?;
            self.db.set_size(id, rows, cols, now_millis())?;
            Ok(())
        } else {
            bail!("session is not live")
        }
    }

    /// Clear the attention flag when the user views/acknowledges a session.
    pub fn acknowledge_attention(&self, id: &str) -> Result<Session> {
        self.db
            .set_attention(id, AttentionState::None, None, now_millis())?;
        self.db
            .get_session(id)?
            .ok_or_else(|| anyhow!("no such session"))
    }

    fn spawn_monitor(self: Arc<Self>, id: String, handle: Arc<dyn BackendSession>, started_at: i64) {
        tokio::spawn(async move {
            let mut status_rx = handle.watch_status();
            let (_snap, mut out_rx) = handle.attach();
            let mut tail = String::new();
            let mut last_activity_write = 0i64;

            loop {
                tokio::select! {
                    changed = status_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        let st = status_rx.borrow().clone();
                        if st.is_terminal() {
                            self.on_exit(&id, &handle, started_at, st).await;
                            break;
                        }
                    }
                    recv = out_rx.recv() => {
                        match recv {
                            Ok(bytes) => {
                                self.on_output(&id, &handle, &bytes, &mut tail, &mut last_activity_write);
                            }
                            Err(RecvError::Lagged(_)) => { /* attention is best-effort */ }
                            Err(RecvError::Closed) => {
                                // Backend gone; the status watch drives the exit.
                            }
                        }
                    }
                }
            }
        });
    }

    fn on_output(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        bytes: &[u8],
        tail: &mut String,
        last_write: &mut i64,
    ) {
        // Maintain a small decoded tail for prompt/approval detection.
        tail.push_str(&String::from_utf8_lossy(bytes));
        if tail.len() > 4096 {
            let cut = tail.len() - 4096;
            *tail = tail.split_off(cut);
        }
        let bell = bytes.contains(&0x07);
        let (attention, reason) = classify_attention(tail, bell);

        let now = now_millis();
        // Debounce activity writes, but always flush on a blocking/approval signal.
        if attention != AttentionState::Activity || now - *last_write >= 400 {
            *last_write = now;
            let _ = self.db.update_activity(
                id,
                handle.last_seq(),
                now,
                attention,
                reason.as_deref(),
            );
        }
    }

    async fn on_exit(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        started_at: i64,
        status: BackendStatus,
    ) {
        let now = now_millis();
        let last_seq = handle.last_seq();

        // Respect an explicit stop/archive already recorded.
        let existing = self.db.get_session(id).ok().flatten();
        let already = existing.as_ref().map(|s| s.status);

        let (final_status, exit_code, attention, reason, exit_label) = match status {
            BackendStatus::Exited(0) => (
                SessionStatus::Exited,
                Some(0),
                AttentionState::None,
                None,
                "exited(0)".to_string(),
            ),
            BackendStatus::Exited(code) => (
                SessionStatus::Exited,
                Some(code),
                AttentionState::Failed,
                Some(format!("exited with code {code}")),
                format!("exited({code})"),
            ),
            BackendStatus::Failed(msg) => (
                SessionStatus::Failed,
                None,
                AttentionState::Failed,
                Some(msg.clone()),
                format!("failed: {msg}"),
            ),
            BackendStatus::Running => return, // not terminal; ignore
        };

        // If the user explicitly stopped/archived, preserve that status.
        let status_to_write = match already {
            Some(SessionStatus::Stopped) => SessionStatus::Stopped,
            Some(SessionStatus::Archived) => SessionStatus::Archived,
            _ => final_status,
        };

        let _ = self
            .db
            .update_status(id, status_to_write, exit_code, now);
        let _ = self
            .db
            .set_attention(id, attention, reason.as_deref(), now);

        // Structural session summary (deterministic metadata, no LLM).
        let summary = SessionSummary {
            id: Uuid::new_v4().to_string(),
            session_id: id.to_string(),
            agent_plugin_id: existing
                .as_ref()
                .map(|s| s.agent_plugin_id.clone())
                .unwrap_or_default(),
            started_at,
            ended_at: now,
            duration_ms: (now - started_at).max(0),
            exit_status: exit_label,
            terminal_event_start: 1,
            terminal_event_end: last_seq,
        };
        if let Err(e) = self.db.insert_summary(&summary) {
            tracing::warn!(session = %id, "failed to write session summary: {e:#}");
        }

        self.live.lock().remove(id);
        tracing::info!(session = %id, status = %status_to_write.as_str(), "session finalized");
    }
}

/// Very small heuristic prompt/approval classifier over the decoded tail.
/// MVP attention detection works on recent output text only — it never needs
/// direct access to sidecar terminal screen state.
fn classify_attention(tail: &str, bell: bool) -> (AttentionState, Option<String>) {
    let lower = tail.to_lowercase();
    const APPROVAL_PATTERNS: &[&str] = &[
        "(y/n)",
        "[y/n]",
        "(yes/no)",
        "do you want to",
        "proceed?",
        "continue? (",
        "overwrite?",
        "password:",
        "passphrase:",
        "are you sure",
        "press enter to continue",
    ];
    for p in APPROVAL_PATTERNS {
        if lower.contains(p) {
            return (
                AttentionState::ApprovalNeeded,
                Some(format!("prompt detected: {p}")),
            );
        }
    }
    if bell {
        return (
            AttentionState::LikelyBlocked,
            Some("terminal bell".to_string()),
        );
    }
    (AttentionState::Activity, None)
}
