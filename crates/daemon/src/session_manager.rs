use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

/// How long output must be silent before a *working* session is considered idle
/// (finished its turn, waiting for the next input).
const IDLE_AFTER: Duration = Duration::from_secs(4);

/// After the user sends input, output arriving within this window is treated as
/// the terminal *echoing* their keystrokes back — not the agent working — so an
/// idle prompt stays idle while you type your next command into it. The window
/// is bypassed once the input submits a line (CR/LF), which does hand control to
/// the agent (see [`Interaction::submitted`]).
const ECHO_WINDOW: Duration = Duration::from_millis(1000);

use crate::backend::{BackendSession, BackendSpawnSpec, BackendStatus, HolderEntry, SessionBackend};
use crate::db::Db;
use crate::domain::{
    AttentionState, Session, SessionStatus, SessionSummary, Workspace, WorkspaceInstance,
};
use crate::plugins::{attention, AgentContext, AgentPlugin, PluginRegistry};
use crate::util::now_millis;
use crate::workspace;

/// A destructive operation was refused because it would discard uncommitted or
/// unmerged work. Carried as a typed error so the API can answer `409 Conflict`
/// and the client can confirm and retry with `force`.
#[derive(Debug)]
pub struct NeedsForce(pub String);

impl std::fmt::Display for NeedsForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NeedsForce {}

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

/// Per-session signal shared from the input path (the API's WebSocket handler)
/// to the monitor task, letting the monitor tell keystroke *echo* apart from the
/// agent actually working. Consistent with the rest of the monitor: cheap atomic
/// stores on the hot input path, read when output arrives.
#[derive(Default)]
struct Interaction {
    /// Set when the user views or answers a session — tells the monitor to clear
    /// a sticky "blocked" (needs-attention) state.
    reset: AtomicBool,
    /// Wall-clock ms of the user's most recent input (`0` = none yet). Output
    /// within [`ECHO_WINDOW`] of this is likely the terminal echoing keystrokes.
    last_input_ms: AtomicI64,
    /// The user's most recent input submitted a line (contained CR/LF), i.e. it
    /// likely handed control to the agent, so its output *is* real work — not
    /// echo. Latched on submit, cleared when the session settles back to idle.
    submitted: AtomicBool,
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
    /// Per-session interaction signals, keyed by session id (see [`Interaction`]).
    interactions: Mutex<HashMap<String, Arc<Interaction>>>,
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
            interactions: Mutex::new(HashMap::new()),
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

        self.clone()
            .spawn_monitor(id.clone(), handle, session.created_at, Some(plugin.clone()));

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

                    // Picking an existing branch that is already checked out
                    // somewhere: share that working tree rather than fail. Git
                    // forbids a second checkout of one branch, and sharing is
                    // exactly what lets two sessions (e.g. plan-with-CC then
                    // review-with-codex) see the same diffs.
                    if let Some(name) = requested {
                        if !req.create_branch {
                            if let Some((existing_path, is_main)) =
                                workspace::worktree_for_branch(&root, name)?
                            {
                                // The repo's own checkout can't become a second
                                // worktree; sharing it is a direct checkout,
                                // which owns no worktree or branch to reclaim.
                                let (path, branch, isolation) = if is_main {
                                    (ws.root_path.clone(), None, "direct")
                                } else {
                                    (existing_path, Some(name.to_string()), "shared")
                                };
                                let inst = WorkspaceInstance {
                                    id: Uuid::new_v4().to_string(),
                                    workspace_id: ws.id.clone(),
                                    session_id: Some(session_id.to_string()),
                                    path: path.clone(),
                                    branch,
                                    isolation: isolation.into(),
                                    status: "active".into(),
                                    created_at: now,
                                };
                                return Ok((path, Some(inst)));
                            }
                        }
                    }

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

    /// Unregister a workspace (removes it from the allowlist). Refuses while it
    /// still has live sessions. Does not stop sessions or delete worktrees on
    /// disk — it only drops the registration; existing session records keep
    /// their (now dangling) `workspace_id`.
    pub fn remove_workspace(&self, id: &str) -> Result<()> {
        let ws = self
            .db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        let has_live = self
            .db
            .list_sessions()?
            .iter()
            .any(|s| s.workspace_id.as_deref() == Some(id) && !s.status.is_terminal());
        if has_live {
            bail!(
                "workspace `{}` still has live sessions; stop them first",
                ws.name
            );
        }
        self.db.delete_workspace(id)?;
        Ok(())
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
        if inst.isolation == "worktree" || inst.isolation == "shared" {
            if self.live_handle(session_id).is_some() {
                bail!("stop the session before cleaning up its worktree");
            }
            // Only reclaim the worktree once the last session sharing it leaves.
            if self.db.count_active_instances_at_path(&inst.path, &inst.id)? == 0 {
                let ws = self
                    .db
                    .get_workspace(&inst.workspace_id)?
                    .ok_or_else(|| anyhow!("workspace record missing"))?;
                workspace::remove_worktree(Path::new(&ws.root_path), Path::new(&inst.path), force)?;
            }
        }
        self.db.set_instance_status(&inst.id, "released")?;
        Ok(())
    }

    /// Find and remove worktrees/branches in a workspace's repo that this daemon
    /// no longer owns — leftovers from throwaway/other daemons that shared the
    /// repo (the "branch already checked out" cause). "Orphaned" = an
    /// `asm-session/*` worktree or branch whose session is unknown to this daemon.
    /// Guards uncommitted (dirty) worktrees and unmerged branches unless `force`.
    pub fn cleanup_orphan_worktrees(
        &self,
        workspace_id: &str,
        force: bool,
    ) -> Result<WorktreeCleanupReport> {
        let ws = self
            .db
            .get_workspace(workspace_id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        if !ws.is_git {
            bail!("workspace `{}` is not a git repository", ws.name);
        }
        let root = Path::new(&ws.root_path);

        // Auto branches are `asm-session/<first 8 chars of the session uuid>`. A
        // worktree/branch whose suffix matches a session this daemon knows about
        // (live or ended) is owned, not orphaned.
        let known: std::collections::HashSet<String> = self
            .db
            .list_sessions()?
            .iter()
            .filter_map(|s| s.id.get(..8).map(str::to_string))
            .collect();

        let mut report = WorktreeCleanupReport::default();

        // 1. Drop registrations whose directories are already gone (always safe).
        let _ = workspace::prune_worktrees(root);

        // 2. Remove orphaned managed worktrees.
        let worktrees = workspace::list_worktrees(root)?;
        for (i, wt) in worktrees.iter().enumerate() {
            if i == 0 {
                continue; // the main worktree
            }
            let Some(branch) = wt.branch.as_deref() else {
                continue; // detached / no branch
            };
            let Some(suffix) = branch.strip_prefix("asm-session/") else {
                continue; // only our auto-managed worktrees
            };
            if known.contains(suffix) {
                continue; // owned by a session we know
            }
            let path = Path::new(&wt.path);
            if !force && workspace::worktree_is_dirty(path) {
                report.skipped_dirty.push(wt.path.clone());
                continue;
            }
            if workspace::remove_worktree(root, path, force).is_ok() {
                report.removed_worktrees.push(wt.path.clone());
                delete_orphan_branch(root, branch, force, &mut report);
            } else {
                report.skipped_dirty.push(wt.path.clone());
            }
        }

        // 3. Orphaned `asm-session/*` branches that have no worktree left.
        let (branches, _head) = workspace::list_branches(root)?;
        for b in branches {
            let Some(suffix) = b.strip_prefix("asm-session/") else {
                continue;
            };
            if known.contains(suffix) || report.deleted_branches.contains(&b) {
                continue;
            }
            delete_orphan_branch(root, &b, force, &mut report);
        }

        Ok(report)
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

    /// Tear down live backend sessions on daemon shutdown. For an in-process
    /// backend this kills each PTY child so nothing leaks. For an out-of-process
    /// holder (asmux) it does the **opposite**: the children must survive the
    /// daemon, so we detach and leave them running (recorded `running`) to be
    /// re-adopted next start — killing them would defeat durability. Returns how
    /// many sessions were actively stopped (0 for a surviving holder).
    pub fn shutdown_all_live(&self) -> usize {
        // Drain under the lock so nothing else can grab a handle mid-shutdown.
        let handles: Vec<(String, Arc<dyn BackendSession>)> =
            self.live.lock().drain().collect();

        if self.backend.keep_sessions_on_shutdown() {
            // Leave the holder's children running; the socket closing on process
            // exit lets asmux reclaim the attachment. Do NOT stop() or mark them
            // stopped — they stay `running` for adopt-on-restart.
            tracing::info!(
                "holder backend: leaving {} live session(s) running for adopt",
                handles.len()
            );
            return 0;
        }

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

    /// Reconcile sessions left live in the DB after a restart.
    ///
    /// - In-process backend: the PTYs are gone, so those rows become `failed`.
    /// - Out-of-process holder: adopt-or-reconcile against `holder_list()` —
    ///   alive in the holder → **adopt** (re-attach, mark `running`); a real exit
    ///   record → `exited`/`failed`; absent from the holder → **`indeterminate`**
    ///   (the holder itself died, so no completion record exists).
    pub async fn startup_reconcile(self: &Arc<Self>) -> Result<()> {
        if !self.backend.keep_sessions_on_shutdown() {
            let n = self.db.reconcile_orphans_on_startup(now_millis())?;
            if n > 0 {
                tracing::warn!(
                    "reconciled {n} session(s) to `failed` after restart (in-process backend not recoverable)"
                );
            }
            return Ok(());
        }

        let holder = match self.backend.holder_list() {
            Ok(h) => h,
            Err(e) => {
                tracing::error!("holder list failed ({e:#}); reconciling live sessions to indeterminate");
                for id in self.db.live_session_ids()? {
                    self.reconcile_indeterminate(&id)?;
                }
                return Ok(());
            }
        };
        let by_id: HashMap<String, HolderEntry> =
            holder.into_iter().map(|h| (h.id.clone(), h)).collect();

        let mut adopted = 0usize;
        let mut reconciled = 0usize;
        for id in self.db.live_session_ids()? {
            let sess = self.db.get_session(&id)?;
            let (rows, cols) = sess.as_ref().map(|s| (s.rows, s.cols)).unwrap_or((24, 80));
            let created_at = sess.as_ref().map(|s| s.created_at).unwrap_or_else(now_millis);
            let plugin = sess
                .as_ref()
                .and_then(|s| self.registry.get(&s.agent_plugin_id));

            match by_id.get(&id) {
                Some(entry) if entry.alive => match self.backend.adopt(&id, rows, cols) {
                    Ok(Some(handle)) => {
                        self.live.lock().insert(id.clone(), handle.clone());
                        self.db
                            .update_status(&id, SessionStatus::Running, None, now_millis())?;
                        self.clone().spawn_monitor(id.clone(), handle, created_at, plugin.clone());
                        adopted += 1;
                        tracing::info!(session = %id, "adopted live holder session");
                    }
                    Ok(None) | Err(_) => {
                        self.reconcile_indeterminate(&id)?;
                        reconciled += 1;
                    }
                },
                Some(entry) => {
                    // The holder has a real completion record.
                    let (status, code) = if entry.exit_signal != 0 {
                        (SessionStatus::Failed, None)
                    } else {
                        (SessionStatus::Exited, Some(entry.exit_code))
                    };
                    self.db.update_status(&id, status, code, now_millis())?;
                    reconciled += 1;
                }
                None => {
                    // Absent from the holder: the holder died → outcome unknown.
                    self.reconcile_indeterminate(&id)?;
                    reconciled += 1;
                }
            }
        }
        tracing::info!("startup reconcile: adopted {adopted}, reconciled {reconciled} session(s)");
        Ok(())
    }

    /// Mark a session `indeterminate` with the acmux-style advisory.
    fn reconcile_indeterminate(&self, id: &str) -> Result<()> {
        let now = now_millis();
        self.db
            .update_status(id, SessionStatus::Indeterminate, None, now)?;
        self.db.set_attention(
            id,
            AttentionState::LikelyBlocked,
            Some("no completion record — the session holder exited while this was running; check the preserved output before assuming it finished"),
            now,
        )?;
        Ok(())
    }

    /// Archive a finished session. Unlike a plain "finished" session (which stays
    /// in history with its branch kept), archiving is the "throw this away" step:
    /// it removes the session's managed worktree and deletes its branch, then
    /// marks the record `archived` (dropped from the history view). Refuses to
    /// discard uncommitted or unmerged work unless `force` — see `discard_instance`.
    pub fn archive_session(&self, id: &str, force: bool) -> Result<Session> {
        let s = self
            .db
            .get_session(id)?
            .ok_or_else(|| anyhow!("no such session"))?;
        if !s.status.is_terminal() {
            bail!("cannot archive a live session; stop it first");
        }
        self.discard_instance(id, force)?;
        self.db
            .update_status(id, SessionStatus::Archived, s.exit_code, now_millis())?;
        self.db
            .get_session(id)?
            .ok_or_else(|| anyhow!("session vanished"))
    }

    /// Tear down a session's managed worktree and delete its branch, reclaiming
    /// both. A no-op for ad-hoc sessions and direct/plain instances (which share
    /// the source checkout and own no branch). Guards against data loss unless
    /// `force`: a dirty worktree or an unmerged branch raises [`NeedsForce`] so
    /// the caller can confirm before anything is removed. The unmerged check runs
    /// before the worktree is touched, so a refusal leaves everything intact.
    fn discard_instance(&self, session_id: &str, force: bool) -> Result<()> {
        let Some(inst) = self.db.get_instance_for_session(session_id)? else {
            return Ok(()); // ad-hoc session: nothing managed to remove
        };
        if inst.isolation != "worktree" && inst.isolation != "shared" {
            return Ok(()); // direct/plain: no owned worktree or branch
        }
        let ws = self
            .db
            .get_workspace(&inst.workspace_id)?
            .ok_or_else(|| anyhow!("workspace record missing"))?;
        let root = Path::new(&ws.root_path);
        let inst_path = Path::new(&inst.path);
        let active = inst.status == "active";

        if active && self.live_handle(session_id).is_some() {
            bail!("stop the session before archiving it");
        }

        // Another session is still working in this shared worktree: relinquish
        // our own claim but leave the directory and branch for the remaining
        // sharer(s). Whoever leaves last (this check returns 0) reclaims both.
        // No `force` bypass — force discards *our* work, never evicts a sharer.
        if self.db.count_active_instances_at_path(&inst.path, &inst.id)? > 0 {
            if active {
                self.db.set_instance_status(&inst.id, "released")?;
            }
            return Ok(());
        }

        // Refuse to silently discard work unless forced. Both guards surface as
        // `NeedsForce` (→ HTTP 409) so the client can confirm and retry.
        if !force {
            if active && workspace::worktree_is_dirty(inst_path) {
                return Err(NeedsForce(
                    "worktree has uncommitted changes; archiving would discard them".into(),
                )
                .into());
            }
            if let Some(branch) = inst.branch.as_deref() {
                if workspace::branch_exists(root, branch)
                    && !workspace::branch_is_merged(root, branch)
                {
                    return Err(NeedsForce(format!(
                        "branch `{branch}` has unmerged commits; archiving would delete them"
                    ))
                    .into());
                }
            }
        }

        // Safe (or forced): drop the worktree first (a branch checked out in a
        // worktree cannot be deleted), then the branch itself.
        if active {
            workspace::remove_worktree(root, inst_path, force)?;
            self.db.set_instance_status(&inst.id, "released")?;
        }
        if let Some(branch) = inst.branch.as_deref() {
            if workspace::branch_exists(root, branch) {
                workspace::delete_branch(root, branch, force)?;
            }
        }
        Ok(())
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
        self.signal_attn_reset(id);
        self.db
            .get_session(id)?
            .ok_or_else(|| anyhow!("no such session"))
    }

    /// The user sent input to a session. Records when (so the monitor can treat
    /// the imminent keystroke echo as *not* the agent working) and whether it
    /// submitted a line, and resolves any pending "blocked" state. Cheap: atomic
    /// stores only; the badge itself updates on the next output or on view.
    pub fn note_interaction(&self, id: &str, data: &[u8]) {
        if let Some(sig) = self.interactions.lock().get(id) {
            sig.last_input_ms.store(now_millis(), Ordering::Relaxed);
            if data.iter().any(|&b| b == b'\r' || b == b'\n') {
                sig.submitted.store(true, Ordering::Relaxed);
            }
            sig.reset.store(true, Ordering::Relaxed);
        }
    }

    fn signal_attn_reset(&self, id: &str) {
        if let Some(sig) = self.interactions.lock().get(id) {
            sig.reset.store(true, Ordering::Relaxed);
        }
    }

    fn spawn_monitor(
        self: Arc<Self>,
        id: String,
        handle: Arc<dyn BackendSession>,
        started_at: i64,
        plugin: Option<Arc<dyn AgentPlugin>>,
    ) {
        let sig = Arc::new(Interaction::default());
        self.interactions.lock().insert(id.clone(), sig.clone());
        tokio::spawn(async move {
            let mut status_rx = handle.watch_status();
            let (_snap, mut out_rx) = handle.attach();
            let mut tail = String::new();
            let mut last_activity_write = 0i64;
            let mut last_attn = AttentionState::None;
            // Carries OSC-escape state across chunks so a window-title update
            // split over two reads isn't miscounted as a bell (see `scan_bell`).
            let mut in_osc = false;

            loop {
                // Only a *working* session needs the close idle watch; a blocked
                // session is sticky (stays until viewed/answered) and silence
                // never demotes it to idle.
                let idle_delay = if last_attn == AttentionState::Activity {
                    IDLE_AFTER
                } else {
                    Duration::from_secs(60)
                };
                let idle_tick = tokio::time::sleep(idle_delay);

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
                                // If the user viewed/answered since the last chunk,
                                // drop a sticky *block* so fresh output reclassifies.
                                // A plain idle prompt is left untouched here — its
                                // keystroke echo must not read as work, which
                                // `on_output` handles via the input timing below.
                                if sig.reset.swap(false, Ordering::Relaxed)
                                    && matches!(
                                        last_attn,
                                        AttentionState::LikelyBlocked
                                            | AttentionState::ApprovalNeeded
                                            | AttentionState::Error
                                    )
                                {
                                    last_attn = AttentionState::None;
                                }
                                let last_input_ms = sig.last_input_ms.load(Ordering::Relaxed);
                                let submitted = sig.submitted.load(Ordering::Relaxed);
                                self.on_output(&id, &handle, &bytes, plugin.as_ref(), &mut tail, &mut last_activity_write, &mut last_attn, &mut in_osc, last_input_ms, submitted);
                            }
                            Err(RecvError::Lagged(_)) => { /* attention is best-effort */ }
                            Err(RecvError::Closed) => {
                                // Backend gone; the status watch drives the exit.
                            }
                        }
                    }
                    _ = idle_tick => {
                        self.on_idle(&id, &handle, plugin.as_ref(), &mut last_attn, &sig);
                    }
                }
            }
            self.interactions.lock().remove(&id);
        });
    }

    /// Output has been silent for [`IDLE_AFTER`]: a *working* session is now idle,
    /// waiting for the next input — unless the agent's screen says it stopped
    /// **on an error** (Claude Code's "API Error: …" prints with no bell and no
    /// prompt, so this settle is the only moment that distinguishes "finished,
    /// waiting" from "died mid-turn"). A blocked/errored session is sticky and
    /// stays that way — silence doesn't mean it stopped needing you.
    fn on_idle(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        plugin: Option<&Arc<dyn AgentPlugin>>,
        last_attn: &mut AttentionState,
        sig: &Interaction,
    ) {
        if *last_attn != AttentionState::Activity {
            return;
        }
        // Fresh idle prompt: whatever the user types next is composing again,
        // so clear the submit latch — their keystroke echo is suppressed until
        // they submit the next line.
        sig.submitted.store(false, Ordering::Relaxed);
        let (state, reason) = match plugin.and_then(|p| p.idle_error(&handle.screen_text())) {
            Some(reason) => (AttentionState::Error, reason),
            None => (AttentionState::Idle, "idle — waiting for input".to_string()),
        };
        *last_attn = state;
        let _ = self.db.set_attention(id, state, Some(&reason), now_millis());
    }

    #[allow(clippy::too_many_arguments)]
    fn on_output(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        bytes: &[u8],
        plugin: Option<&Arc<dyn AgentPlugin>>,
        tail: &mut String,
        last_write: &mut i64,
        last_attn: &mut AttentionState,
        in_osc: &mut bool,
        last_input_ms: i64,
        submitted: bool,
    ) {
        // Maintain a small decoded tail for the default (tail-based) classifier.
        tail.push_str(&String::from_utf8_lossy(bytes));
        trim_tail(tail, 4096);
        // Only trust the bell as an attention signal for agents that opt in
        // (a plain shell rings it as UI noise), and only a *real* bell — not the
        // BEL that terminates an OSC window-title update, which agents like
        // Claude Code emit constantly while working (`ESC ] 0 ; <title> BEL`).
        let bell = plugin.is_some_and(|p| p.bell_means_attention()) && scan_bell(bytes, in_osc);
        // Classification is per-provider. Most agents read the raw output tail;
        // one whose approval UI the tail can't see (Claude Code's boxed menu)
        // asks for the rendered screen instead — bounded to the visible grid and
        // always current, so a prompt buried above a footer / redraw frames is
        // still seen. An unknown plugin falls back to the default heuristic.
        let screen;
        let (raw, reason) = match plugin {
            Some(p) if p.attention_uses_screen() => {
                screen = handle.screen_text();
                p.attention(&screen, bell)
            }
            Some(p) => p.attention(tail, bell),
            None => attention::default_attention(tail, bell),
        };
        let now = now_millis();

        // Keystroke echo at an idle prompt: the user is composing their next
        // command, the agent is not working. Output that lands within
        // [`ECHO_WINDOW`] of their last input — and that hasn't yet submitted a
        // line — is that echo, so the prompt stays idle. A submit (CR/LF) hands
        // off to the agent, so its output *is* real work and falls through. This
        // only guards the idle state; spontaneous agent output (no recent input)
        // is outside the window and reads as activity as before.
        if raw == AttentionState::Activity
            && *last_attn == AttentionState::Idle
            && !submitted
            && last_input_ms != 0
            && now.saturating_sub(last_input_ms) < ECHO_WINDOW.as_millis() as i64
        {
            return;
        }

        // Sticky "blocked"/"error": agents ring the bell / show a prompt when
        // they need you, then keep redrawing (TUIs) — plain redraw output (or a
        // still-running background shell's noise under a dead turn) must NOT
        // demote that back to "working". It clears when the user views or
        // answers (which resets `last_attn` in the monitor loop).
        let was_blocked = matches!(
            *last_attn,
            AttentionState::LikelyBlocked | AttentionState::ApprovalNeeded | AttentionState::Error
        );
        let attention = if raw == AttentionState::Activity && was_blocked {
            *last_attn
        } else {
            raw
        };
        *last_attn = attention;

        // Debounce activity writes, but always flush a blocking/approval signal.
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

        // A user-ended session is not a failure. Stopping kills the child (a
        // non-zero/​signalled exit), which would otherwise show as `failed` with a
        // scary exit code — clear both and label the summary by the user action.
        let user_ended = matches!(
            status_to_write,
            SessionStatus::Stopped | SessionStatus::Archived
        );
        let (exit_code, attention, reason, exit_label) = if user_ended {
            (None, AttentionState::None, None, status_to_write.as_str().to_string())
        } else {
            (exit_code, attention, reason, exit_label)
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

/// What an orphan-worktree cleanup removed and what it left (needing `force`).
#[derive(Debug, Default, Serialize)]
pub struct WorktreeCleanupReport {
    /// Worktree directories removed (orphaned, unknown to this daemon).
    pub removed_worktrees: Vec<String>,
    /// Orphaned `asm-session/*` branches deleted.
    pub deleted_branches: Vec<String>,
    /// Worktrees left because they have uncommitted changes (retry with force).
    pub skipped_dirty: Vec<String>,
    /// Branches left because they have unmerged commits (retry with force).
    pub skipped_unmerged: Vec<String>,
}

/// Delete an orphaned branch when it is safe (fully merged) or forced. Records
/// the outcome in `report`.
fn delete_orphan_branch(
    root: &Path,
    branch: &str,
    force: bool,
    report: &mut WorktreeCleanupReport,
) {
    if force || workspace::branch_is_merged(root, branch) {
        if workspace::delete_branch(root, branch, force).is_ok() {
            report.deleted_branches.push(branch.to_string());
        }
    } else if !report.skipped_unmerged.iter().any(|b| b == branch) {
        report.skipped_unmerged.push(branch.to_string());
    }
}

/// Scan an output chunk for a *genuine* terminal bell (a standalone `0x07`),
/// ignoring the BEL that terminates an OSC control string (`ESC ] … BEL`).
/// Agents like Claude Code set the window title to their current task on every
/// redraw — `ESC ] 0 ; <title> BEL` — so a naive `contains(0x07)` reads those
/// title updates as attention bells and pins a working session to "blocked".
///
/// OSC strings end at either BEL or ST (`ESC \`). `in_osc` carries the parser
/// state across chunk boundaries so a title split over two reads (its `ESC ]`
/// in one chunk, its BEL in the next) still isn't miscounted. Only a BEL seen
/// outside an OSC string counts as a real bell.
fn scan_bell(bytes: &[u8], in_osc: &mut bool) -> bool {
    let mut bell = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if *in_osc {
            if b == 0x07 {
                *in_osc = false; // OSC terminator — not an attention bell.
            } else if b == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
                *in_osc = false; // ST terminator (ESC \).
                i += 1;
            }
        } else if b == 0x1b && bytes.get(i + 1) == Some(&b']') {
            *in_osc = true; // OSC introducer (ESC ]).
            i += 1;
        } else if b == 0x07 {
            bell = true; // a real, standalone bell.
        }
        i += 1;
    }
    bell
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

    // Attention classification lives in `plugins::attention` (per-provider); its
    // unit tests are colocated there. This module keeps the byte-stream
    // mechanics: bell scanning and tail trimming.

    // ---- bell scanning (OSC title vs. real bell) ----

    #[test]
    fn osc_title_bel_is_not_a_bell() {
        // The real bug: Claude Code sets the window title to its current task on
        // every redraw via `ESC ] 0 ; <title> BEL`. That BEL must NOT read as an
        // attention bell, or an actively-working session shows as "blocked".
        let mut in_osc = false;
        let title = b"\x1b]0;Design multi-hop private network architecture\x07";
        assert!(!scan_bell(title, &mut in_osc));
        assert!(!in_osc, "a complete OSC leaves us outside OSC state");
    }

    #[test]
    fn standalone_bel_is_a_bell() {
        let mut in_osc = false;
        assert!(scan_bell(b"work\x07more", &mut in_osc));
        // A real bell after a title update still registers.
        assert!(scan_bell(b"\x1b]0;title\x07 done\x07", &mut false));
    }

    #[test]
    fn st_terminated_osc_is_not_a_bell() {
        // OSC may also end with ST (`ESC \`) rather than BEL; a following real
        // bell must still count.
        let mut in_osc = false;
        assert!(!scan_bell(b"\x1b]0;title\x1b\\", &mut in_osc));
        assert!(!in_osc);
        assert!(scan_bell(b"\x1b]0;t\x1b\\\x07", &mut false));
    }

    #[test]
    fn osc_title_split_across_chunks_is_not_a_bell() {
        // A title update whose BEL lands in the next read: the carried `in_osc`
        // state keeps the terminator from being miscounted.
        let mut in_osc = false;
        assert!(!scan_bell(b"\x1b]0;Design multi-hop", &mut in_osc));
        assert!(in_osc, "unterminated OSC carries into the next chunk");
        assert!(!scan_bell(b" network architecture\x07", &mut in_osc));
        assert!(!in_osc);
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
        /// Canned rendered screen returned by `screen_text` (for screen-based
        /// attention tests); empty for the byte-stream mocks.
        screen: String,
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
        fn screen_text(&self) -> String {
            self.screen.clone()
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
                screen: String::new(),
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
        assert!(manager.archive_session(&s.id, false).is_err());
        // After stop it can.
        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let archived = manager.archive_session(&s.id, false).unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Init a git repo with one commit so it can host managed worktrees.
    fn git_init(path: &Path) {
        let run = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(path)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        run(&["commit", "-q", "--allow-empty", "-m", "init"]);
    }

    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn ws_req(workspace_id: &str) -> CreateSessionRequest {
        let mut r = shell_req();
        r.workspace_id = Some(workspace_id.to_string());
        r
    }

    #[tokio::test]
    async fn archive_removes_worktree_and_branch() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        // Managed-worktree session on an auto branch off the clean HEAD.
        let s = manager.create_session(ws_req(&ws.id)).unwrap();
        let inst = manager.get_instance_for_session(&s.id).unwrap().unwrap();
        let branch = inst.branch.clone().unwrap();
        assert!(Path::new(&inst.path).is_dir());
        assert!(workspace::branch_exists(&repo, &branch));

        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // Branch is merged (points at HEAD) and worktree is clean → archives
        // without force, taking the worktree and branch with it.
        let archived = manager.archive_session(&s.id, false).unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
        assert!(!Path::new(&inst.path).exists());
        assert!(!workspace::branch_exists(&repo, &branch));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn archive_guards_unmerged_branch_until_forced() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        let s = manager.create_session(ws_req(&ws.id)).unwrap();
        let inst = manager.get_instance_for_session(&s.id).unwrap().unwrap();
        let branch = inst.branch.clone().unwrap();
        let wt = Path::new(&inst.path);

        // Add a commit on the session branch so it is no longer merged into HEAD.
        git_in(wt, &["commit", "-q", "--allow-empty", "-m", "work"]);

        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // Non-force archive refuses (unmerged) with a NeedsForce error, leaving
        // the worktree, branch, and status untouched.
        let err = manager.archive_session(&s.id, false).unwrap_err();
        assert!(err.downcast_ref::<NeedsForce>().is_some());
        assert!(workspace::branch_exists(&repo, &branch));
        assert!(wt.is_dir());
        assert_eq!(
            manager.get_session(&s.id).unwrap().unwrap().status,
            SessionStatus::Stopped
        );

        // Forced archive removes both.
        let archived = manager.archive_session(&s.id, true).unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
        assert!(!wt.exists());
        assert!(!workspace::branch_exists(&repo, &branch));

        let _ = std::fs::remove_dir_all(dir);
    }

    // ---- keystroke echo vs. real work (idle-prompt accuracy) ----

    fn mock_handle() -> Arc<dyn BackendSession> {
        mock_handle_with_screen("")
    }

    fn mock_handle_with_screen(screen: &str) -> Arc<dyn BackendSession> {
        let (tx, _) = broadcast::channel(16);
        let (status_tx, status_rx) = watch::channel(BackendStatus::Running);
        Arc::new(MockSession {
            tx,
            status_tx,
            status_rx,
            seq: AtomicU64::new(0),
            screen: screen.to_string(),
        })
    }

    #[test]
    fn typing_at_idle_prompt_stays_idle() {
        // The bug: at an idle prompt, the PTY echoes each keystroke as output,
        // which used to read as the agent "working". A keystroke echo (recent
        // input, no line submitted) must leave the prompt idle — and write
        // nothing (`last_write` untouched).
        let (manager, dir) = test_manager();
        let handle = mock_handle();
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Idle);
        manager.on_output(
            "sid", &handle, b"l", None, &mut tail, &mut last_write, &mut last_attn,
            &mut false, now_millis(), false,
        );
        assert_eq!(last_attn, AttentionState::Idle);
        assert_eq!(last_write, 0, "echo must not record activity");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn submitting_a_line_from_idle_reads_as_working() {
        // Pressing Enter (input contained CR/LF -> `submitted`) hands off to the
        // agent, so its output is real work, not echo.
        let (manager, dir) = test_manager();
        let handle = mock_handle();
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Idle);
        manager.on_output(
            "sid", &handle, b"thinking...", None, &mut tail, &mut last_write,
            &mut last_attn, &mut false, now_millis(), true,
        );
        assert_eq!(last_attn, AttentionState::Activity);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn spontaneous_output_after_idle_reads_as_working() {
        // Output with no recent input (e.g. the agent resumed on its own) is
        // outside the echo window, so it must read as working — the echo guard
        // only covers keystrokes the user just typed.
        let (manager, dir) = test_manager();
        let handle = mock_handle();
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Idle);
        let stale_input = now_millis() - ECHO_WINDOW.as_millis() as i64 - 500;
        manager.on_output(
            "sid", &handle, b"progress 40%", None, &mut tail, &mut last_write,
            &mut last_attn, &mut false, stale_input, false,
        );
        assert_eq!(last_attn, AttentionState::Activity);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn echo_guard_does_not_mask_an_approval_prompt() {
        // If what lands at the idle prompt is itself an approval prompt, that is
        // a *block* — the echo timing must not swallow it.
        let (manager, dir) = test_manager();
        let handle = mock_handle();
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Idle);
        manager.on_output(
            "sid", &handle, b"Proceed? (y/n)", None, &mut tail, &mut last_write,
            &mut last_attn, &mut false, now_millis(), false,
        );
        assert_eq!(last_attn, AttentionState::ApprovalNeeded);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn claude_screen_prompt_blocks_via_on_output() {
        // End-to-end wiring: the Claude plugin opts into screen-based detection,
        // so `on_output` must classify from the handle's rendered screen (not the
        // raw byte tail). The byte chunk here is a spinner-frame redraw — exactly
        // what the tail sees while the prompt sits above a footer — yet the screen
        // shows the approval menu, so it must read as ApprovalNeeded.
        let (manager, dir) = test_manager();
        let claude = manager.registry.get("claude").unwrap();
        let screen = " Do you want to proceed?\n \u{276f} 1. Yes\n   2. No\n\n Esc to cancel \u{b7} Tab to amend";
        let handle = mock_handle_with_screen(screen);
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Activity);
        manager.on_output(
            "sid", &handle, b"\x1b[35B\xe2\x97\x8f", Some(&claude), &mut tail,
            &mut last_write, &mut last_attn, &mut false, 0, false,
        );
        assert_eq!(last_attn, AttentionState::ApprovalNeeded);
        let _ = std::fs::remove_dir_all(dir);
    }

    // ---- stalled-on-error settle (working -> error, not idle) ----

    #[test]
    fn idle_settle_with_stalled_error_screen_reads_as_error() {
        // The reported bug: the API died mid-turn, Claude printed "API Error: …"
        // and froze — no bell, no prompt — and the silence timer settled the
        // session to a calm "idle". The settle must consult the plugin's screen
        // check and land on Error instead.
        let (manager, dir) = test_manager();
        let claude = manager.registry.get("claude").unwrap();
        let screen = "\
\u{25cf} API Error: Server error mid-response. The response above may be incomplete.\n\
\n\
✻ Worked for 32m 22s\n\
\n\
────────────────────\n\
\u{276f} \n\
────────────────────\n\
  ⏵⏵ bypass permissions on · ← for agents";
        let handle = mock_handle_with_screen(screen);
        let sig = Interaction::default();
        let mut last_attn = AttentionState::Activity;
        manager.on_idle("sid", &handle, Some(&claude), &mut last_attn, &sig);
        assert_eq!(last_attn, AttentionState::Error);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn idle_settle_with_clean_screen_reads_as_idle() {
        let (manager, dir) = test_manager();
        let claude = manager.registry.get("claude").unwrap();
        let screen = "\
\u{25cf} Done. All tests pass.\n\
\n\
✻ Worked for 3s\n\
\n\
────────────────────\n\
\u{276f} \n\
────────────────────\n\
  ? for shortcuts";
        let handle = mock_handle_with_screen(screen);
        let sig = Interaction::default();
        let mut last_attn = AttentionState::Activity;
        manager.on_idle("sid", &handle, Some(&claude), &mut last_attn, &sig);
        assert_eq!(last_attn, AttentionState::Idle);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn background_noise_does_not_clear_error() {
        // The captured stall had "1 shell still running": a background
        // command's later output under a dead turn must not demote the error
        // back to "working" — it is sticky until the user views or types.
        let (manager, dir) = test_manager();
        let handle = mock_handle();
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Error);
        manager.on_output(
            "sid", &handle, b"bg command output", None, &mut tail, &mut last_write,
            &mut last_attn, &mut false, 0, false,
        );
        assert_eq!(last_attn, AttentionState::Error);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn existing_branch_shares_worktree() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        // First session gets its own managed worktree on an auto branch.
        let a = manager.create_session(ws_req(&ws.id)).unwrap();
        let inst_a = manager.get_instance_for_session(&a.id).unwrap().unwrap();
        let branch = inst_a.branch.clone().unwrap();
        assert_eq!(inst_a.isolation, "worktree");

        // Second session pointed at that branch shares the first's worktree
        // instead of failing with git's "already checked out".
        let mut req = ws_req(&ws.id);
        req.branch = Some(branch.clone());
        req.create_branch = false;
        let b = manager.create_session(req).unwrap();
        let inst_b = manager.get_instance_for_session(&b.id).unwrap().unwrap();

        assert_eq!(inst_b.isolation, "shared");
        assert_eq!(inst_b.path, inst_a.path, "sharer runs in the owner's worktree");
        assert_eq!(inst_b.branch.as_deref(), Some(branch.as_str()));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn shared_worktree_survives_until_last_session_leaves() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        let a = manager.create_session(ws_req(&ws.id)).unwrap();
        let inst_a = manager.get_instance_for_session(&a.id).unwrap().unwrap();
        let branch = inst_a.branch.clone().unwrap();
        let wt = inst_a.path.clone();

        let mut req = ws_req(&ws.id);
        req.branch = Some(branch.clone());
        let b = manager.create_session(req).unwrap();
        assert_eq!(
            manager.get_instance_for_session(&b.id).unwrap().unwrap().path,
            wt
        );

        // Archive the owner first while the sharer is still active: the shared
        // worktree and branch must survive — the sharer is still using them.
        manager.stop_session(&a.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        manager.archive_session(&a.id, false).unwrap();
        assert!(Path::new(&wt).is_dir(), "worktree kept while a sharer lives");
        assert!(
            workspace::branch_exists(&repo, &branch),
            "branch kept while a sharer lives"
        );

        // The last session out reclaims both.
        manager.stop_session(&b.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        manager.archive_session(&b.id, false).unwrap();
        assert!(!Path::new(&wt).exists(), "last session removes the worktree");
        assert!(
            !workspace::branch_exists(&repo, &branch),
            "last session deletes the branch"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
