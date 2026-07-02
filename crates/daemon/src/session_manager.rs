use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::backend::{BackendSession, BackendSpawnSpec, BackendStatus, SessionBackend};
use crate::db::Db;
use crate::domain::{
    AttentionState, Session, SessionStatus, SessionSummary, Workspace, WorkspaceInstance,
};
use crate::plugins::{AgentContext, PluginRegistry};
use crate::util::now_millis;
use crate::workspace;

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
    /// Run directly in the source checkout instead of an isolated worktree.
    pub direct_checkout: bool,
    /// Branch for the isolated worktree. `None` auto-generates an app-managed
    /// branch (the default). Otherwise it is created or checked out per
    /// `create_branch`.
    pub branch: Option<String>,
    /// When `branch` is set: `true` creates it off `base_ref`, `false` checks
    /// out an existing branch of that name.
    pub create_branch: bool,
    /// Start point for a newly created branch (branch/tag/commit). `None` = HEAD.
    pub base_ref: Option<String>,
    /// Selected agent-option toggles (see `AgentPlugin::options`).
    pub options: Vec<(String, bool)>,
}

/// Owns session lifecycle: plugin resolution, backend spawn, persistence, and
/// the per-session monitor task that tracks exit, summaries, and attention.
pub struct SessionManager {
    pub db: Db,
    pub registry: Arc<PluginRegistry>,
    backend: Arc<dyn SessionBackend>,
    live: Mutex<HashMap<String, Arc<dyn BackendSession>>>,
    /// Base directory under which per-session Git worktrees are created.
    worktree_root: PathBuf,
}

impl SessionManager {
    pub fn new(
        db: Db,
        registry: Arc<PluginRegistry>,
        backend: Arc<dyn SessionBackend>,
        worktree_root: PathBuf,
    ) -> Self {
        Self {
            db,
            registry,
            backend,
            live: Mutex::new(HashMap::new()),
            worktree_root,
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

        let id = Uuid::new_v4().to_string();

        // Resolve the working directory and (optionally) an isolated instance.
        let (resolved_cwd, instance) = self.resolve_workspace(&id, &req)?;

        let ctx = AgentContext {
            command: req.command.clone(),
            extra_args: req.args.clone(),
            extra_env: req.env.clone(),
            options: req.options.clone(),
        };
        let launch = plugin.build_launch(&ctx)?;

        if launch.requires_approval && !req.approve_custom {
            bail!("launch requires explicit approval (custom command)");
        }

        // A session is "risky" if it enabled any of the plugin's danger toggles
        // (e.g. skip-permissions / bypass-sandbox), so the UI can badge it.
        let risky = plugin
            .options()
            .iter()
            .any(|o| o.danger && ctx.opt(&o.key));

        if !Path::new(&resolved_cwd).is_dir() {
            bail!("working directory does not exist: {resolved_cwd}");
        }

        let now = now_millis();
        let session = Session {
            id: id.clone(),
            agent_plugin_id: plugin.id().to_string(),
            command: launch.command.clone(),
            args: launch.args.clone(),
            env: launch.env.clone(),
            working_directory: resolved_cwd.clone(),
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
            risky,
        };
        self.db.insert_session(&session)?;
        if let Some(inst) = &instance {
            self.db.insert_instance(inst)?;
        }

        let spec = BackendSpawnSpec {
            session_id: id.clone(),
            command: launch.command,
            args: launch.args,
            env: launch.env,
            cwd: resolved_cwd,
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

    /// Decide where a session runs: an isolated worktree for a Git workspace,
    /// the source root for a direct/plain workspace, or a raw allowlisted path.
    fn resolve_workspace(
        &self,
        session_id: &str,
        req: &CreateSessionRequest,
    ) -> Result<(String, Option<WorkspaceInstance>)> {
        let now = now_millis();
        match &req.workspace_id {
            Some(ws_id) => {
                let ws = self
                    .db
                    .get_workspace(ws_id)?
                    .ok_or_else(|| anyhow!("unknown workspace `{ws_id}`"))?;
                let root = PathBuf::from(&ws.root_path);
                if !root.is_dir() {
                    bail!("workspace root does not exist: {}", ws.root_path);
                }

                if ws.is_git && !req.direct_checkout {
                    // Isolated managed worktree. The caller may select an
                    // existing branch, name a new one, or let us auto-generate.
                    let instance_path = self.worktree_root.join(session_id);
                    let auto = format!("asm-session/{}", &session_id[..8.min(session_id.len())]);
                    let requested = req
                        .branch
                        .as_deref()
                        .map(str::trim)
                        .filter(|b| !b.is_empty());
                    let base = req
                        .base_ref
                        .as_deref()
                        .map(str::trim)
                        .filter(|b| !b.is_empty())
                        .unwrap_or("HEAD");
                    let spec = match requested {
                        Some(name) if req.create_branch => {
                            workspace::BranchSpec::New { name, base }
                        }
                        Some(name) => workspace::BranchSpec::Existing { name },
                        None => workspace::BranchSpec::Auto { name: &auto },
                    };
                    let branch = workspace::create_worktree(&root, &instance_path, spec)?;
                    let path = instance_path.to_string_lossy().into_owned();
                    let inst = WorkspaceInstance {
                        id: Uuid::new_v4().to_string(),
                        workspace_id: ws.id.clone(),
                        session_id: Some(session_id.to_string()),
                        path: path.clone(),
                        branch,
                        isolation: "worktree".into(),
                        status: "active".into(),
                        created_at: now,
                    };
                    Ok((path, Some(inst)))
                } else {
                    // Direct source checkout (git override) or plain folder.
                    let isolation = if ws.is_git { "direct" } else { "plain" };
                    let inst = WorkspaceInstance {
                        id: Uuid::new_v4().to_string(),
                        workspace_id: ws.id.clone(),
                        session_id: Some(session_id.to_string()),
                        path: ws.root_path.clone(),
                        branch: None,
                        isolation: isolation.into(),
                        status: "active".into(),
                        created_at: now,
                    };
                    Ok((ws.root_path, Some(inst)))
                }
            }
            None => {
                if req.cwd.trim().is_empty() {
                    bail!("cwd is required when no workspace is selected");
                }
                // Raw path: enforce the allowlist once any workspace is registered.
                let workspaces = self.db.list_workspaces()?;
                if !workspaces.is_empty() {
                    let cwd_abs = canonical(&req.cwd);
                    let allowed = workspaces
                        .iter()
                        .any(|w| cwd_abs.starts_with(canonical(&w.root_path)));
                    if !allowed {
                        bail!("working directory is outside all registered workspace roots");
                    }
                }
                Ok((req.cwd.clone(), None))
            }
        }
    }

    pub fn register_workspace(&self, name: String, root_path: String) -> Result<Workspace> {
        let root = PathBuf::from(&root_path);
        if !root.is_dir() {
            bail!("root path is not a directory: {root_path}");
        }
        let canonical_root = canonical(&root_path).to_string_lossy().into_owned();
        let is_git = workspace::is_git_repo(&root);
        let w = Workspace {
            id: Uuid::new_v4().to_string(),
            name,
            root_path: canonical_root,
            is_git,
            created_at: now_millis(),
        };
        self.db.insert_workspace(&w)?;
        Ok(w)
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        self.db.list_workspaces()
    }

    /// Local branches and current HEAD for a workspace, for the new-session
    /// branch picker. Empty for non-Git workspaces.
    pub fn list_workspace_branches(&self, id: &str) -> Result<(Vec<String>, Option<String>)> {
        let w = self
            .db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        if !w.is_git {
            return Ok((vec![], None));
        }
        workspace::list_branches(Path::new(&w.root_path))
    }

    pub fn init_workspace_git(&self, id: &str) -> Result<Workspace> {
        let w = self
            .db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        if w.is_git {
            return Ok(w);
        }
        workspace::init_repo(Path::new(&w.root_path))?;
        self.db.set_workspace_git(id, true)?;
        self.db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("workspace vanished"))
    }

    pub fn get_instance_for_session(&self, session_id: &str) -> Result<Option<WorkspaceInstance>> {
        self.db.get_instance_for_session(session_id)
    }

    /// Remove a session's managed worktree. Guards against dirty worktrees and
    /// live sessions unless `force`.
    pub fn cleanup_instance(&self, session_id: &str, force: bool) -> Result<()> {
        let inst = self
            .db
            .get_instance_for_session(session_id)?
            .ok_or_else(|| anyhow!("no workspace instance for session"))?;
        if inst.status == "released" {
            return Ok(());
        }
        if inst.isolation == "worktree" {
            if self.live_handle(session_id).is_some() {
                bail!("stop the session before cleaning up its worktree");
            }
            let ws = self
                .db
                .get_workspace(&inst.workspace_id)?
                .ok_or_else(|| anyhow!("workspace record missing"))?;
            workspace::remove_worktree(Path::new(&ws.root_path), Path::new(&inst.path), force)?;
        }
        self.db.set_instance_status(&inst.id, "released")?;
        Ok(())
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

    /// Tear down every live backend session. Called on daemon shutdown so no
    /// child process is leaked — the native backend kills its PTY child, and a
    /// future out-of-process/tmux backend would kill its sidecar the same way
    /// through `BackendSession::stop`. Returns how many sessions were stopped.
    pub fn shutdown_all_live(&self) -> usize {
        // Drain under the lock so nothing else can grab a handle mid-shutdown.
        let handles: Vec<(String, Arc<dyn BackendSession>)> =
            self.live.lock().drain().collect();
        let n = handles.len();
        for (id, h) in &handles {
            // Record intent first (like stop_session) so a racing monitor keeps
            // `stopped` rather than reconciling to `failed`.
            let _ = self
                .db
                .update_status(id, SessionStatus::Stopped, None, now_millis());
            if let Err(e) = h.stop() {
                tracing::warn!(session = %id, "failed to stop session on shutdown: {e}");
            }
        }
        n
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
        trim_tail(tail, 4096);
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

/// Best-effort path canonicalization for allowlist comparisons.
fn canonical(p: &str) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p))
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

/// Bound `tail` to at most `max` bytes by dropping the oldest content.
/// Trims only at a UTF-8 char boundary: a raw byte offset can land in the
/// middle of a multi-byte character and panic `String::split_off`. Because we
/// keep the newest bytes, we advance the cut point forward (yielding slightly
/// fewer than `max` bytes when a boundary straddles the cut), which is always
/// safe since `tail.len()` is itself a valid boundary.
fn trim_tail(tail: &mut String, max: usize) {
    if tail.len() <= max {
        return;
    }
    let mut cut = tail.len() - max;
    while cut < tail.len() && !tail.is_char_boundary(cut) {
        cut += 1;
    }
    *tail = tail.split_off(cut);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Snapshot;
    use std::sync::atomic::AtomicU64;
    use tokio::sync::{broadcast, watch};

    // ---- attention classifier ----

    #[test]
    fn detects_approval_prompt() {
        let (a, reason) = classify_attention("Proceed? (y/n)", false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
        assert!(reason.is_some());
    }

    #[test]
    fn bell_is_likely_blocked() {
        let (a, _) = classify_attention("just some output", true);
        assert_eq!(a, AttentionState::LikelyBlocked);
    }

    #[test]
    fn plain_output_is_activity() {
        let (a, reason) = classify_attention("building project...", false);
        assert_eq!(a, AttentionState::Activity);
        assert!(reason.is_none());
    }

    // ---- tail trimming ----

    #[test]
    fn trim_tail_leaves_short_input_untouched() {
        let mut tail = "short".to_string();
        trim_tail(&mut tail, 4096);
        assert_eq!(tail, "short");
    }

    #[test]
    fn trim_tail_does_not_split_multibyte_chars() {
        // "€" is 3 bytes (0xE2 0x82 0xAC). Build a string whose byte length
        // makes the naive cut (len - max) land inside a "€", the exact case
        // that panicked `split_off` with `is_char_boundary` in production.
        let mut tail = "€".repeat(2000); // 6000 bytes
        trim_tail(&mut tail, 4096);
        assert!(tail.len() <= 4096);
        // Result is still valid UTF-8 made only of whole "€"s.
        assert!(tail.chars().all(|c| c == '€'));
        assert_eq!(tail.len() % 3, 0);
    }

    #[test]
    fn trim_tail_keeps_the_newest_bytes() {
        let mut tail = "a".repeat(4096); // ASCII filler
        tail.push_str("TAILEND");
        trim_tail(&mut tail, 4096);
        assert!(tail.len() <= 4096);
        assert!(tail.ends_with("TAILEND"));
    }

    // ---- mock backend proving the SessionBackend boundary ----

    struct MockSession {
        tx: broadcast::Sender<Arc<[u8]>>,
        status_tx: watch::Sender<BackendStatus>,
        status_rx: watch::Receiver<BackendStatus>,
        seq: AtomicU64,
    }

    impl BackendSession for MockSession {
        fn attach(&self) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>) {
            (self.snapshot(), self.tx.subscribe())
        }
        fn snapshot(&self) -> Snapshot {
            Snapshot {
                rows: 24,
                cols: 80,
                repaint: Arc::from(Vec::new().into_boxed_slice()),
                last_seq: self.seq.load(std::sync::atomic::Ordering::SeqCst),
            }
        }
        fn send_input(&self, _data: &[u8]) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _rows: u16, _cols: u16) -> Result<()> {
            Ok(())
        }
        fn stop(&self) -> Result<()> {
            let _ = self.status_tx.send(BackendStatus::Exited(0));
            Ok(())
        }
        fn watch_status(&self) -> watch::Receiver<BackendStatus> {
            self.status_rx.clone()
        }
        fn last_seq(&self) -> u64 {
            self.seq.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    struct MockBackend;

    impl SessionBackend for MockBackend {
        fn id(&self) -> &'static str {
            "mock"
        }
        fn create(&self, _spec: BackendSpawnSpec) -> Result<Arc<dyn BackendSession>> {
            let (tx, _) = broadcast::channel(16);
            let (status_tx, status_rx) = watch::channel(BackendStatus::Running);
            Ok(Arc::new(MockSession {
                tx,
                status_tx,
                status_rx,
                seq: AtomicU64::new(0),
            }))
        }
    }

    fn test_manager() -> (Arc<SessionManager>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("asm-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("test.sqlite3")).unwrap();
        let registry = Arc::new(PluginRegistry::with_builtins());
        let manager = Arc::new(SessionManager::new(
            db,
            registry,
            Arc::new(MockBackend),
            dir.join("worktrees"),
        ));
        (manager, dir)
    }

    fn shell_req() -> CreateSessionRequest {
        CreateSessionRequest {
            agent_plugin_id: "shell".into(),
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            command: None,
            args: vec![],
            env: vec![],
            rows: 24,
            cols: 80,
            workspace_id: None,
            approve_custom: false,
            direct_checkout: false,
            branch: None,
            create_branch: false,
            base_ref: None,
            options: vec![],
        }
    }

    #[tokio::test]
    async fn stop_marks_stopped_and_writes_summary() {
        let (manager, dir) = test_manager();

        let s = manager.create_session(shell_req()).unwrap();
        assert_eq!(s.status, SessionStatus::Running);

        let stopped = manager.stop_session(&s.id).unwrap();
        assert_eq!(stopped.status, SessionStatus::Stopped);

        // Let the monitor task observe the backend exit and finalize.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

        let reloaded = manager.get_session(&s.id).unwrap().unwrap();
        assert_eq!(reloaded.status, SessionStatus::Stopped);

        let summary = manager.get_summary(&s.id).unwrap();
        assert!(summary.is_some(), "a structural summary must be written");
        assert_eq!(summary.unwrap().session_id, s.id);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn shutdown_stops_all_live_sessions() {
        let (manager, dir) = test_manager();

        let a = manager.create_session(shell_req()).unwrap();
        let b = manager.create_session(shell_req()).unwrap();
        assert_eq!(manager.live_count(), 2);

        // Simulate daemon shutdown: every live session must be torn down.
        let stopped = manager.shutdown_all_live();
        assert_eq!(stopped, 2);
        assert_eq!(manager.live_count(), 0, "no live handle may leak");

        // Both are recorded terminal (stopped), so a restart won't need to
        // reconcile them to `failed`.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        for id in [&a.id, &b.id] {
            let s = manager.get_session(id).unwrap().unwrap();
            assert_eq!(s.status, SessionStatus::Stopped);
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn custom_command_without_approval_is_rejected() {
        let (manager, dir) = test_manager();
        let mut req = shell_req();
        req.agent_plugin_id = "custom_command".into();
        req.command = Some("echo hi".into());
        req.approve_custom = false;

        let err = manager.create_session(req).unwrap_err();
        assert!(err.to_string().contains("approval"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn archive_requires_terminal_state() {
        let (manager, dir) = test_manager();
        let s = manager.create_session(shell_req()).unwrap();
        // Live session cannot be archived.
        assert!(manager.archive_session(&s.id).is_err());
        // After stop it can.
        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let archived = manager.archive_session(&s.id).unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
        let _ = std::fs::remove_dir_all(dir);
    }
}
