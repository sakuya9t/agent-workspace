use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
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

/// How often a live session is checked for the agent's own conversation id, until
/// one is found. An agent writes its transcript shortly *after* it starts, so this
/// cannot be a single attempt at spawn — but once captured it is never re-derived,
/// and the polling stops. See [`SessionManager::capture_native_id`].
const CAPTURE_EVERY: Duration = Duration::from_secs(5);

use crate::backend::{
    BackendSession, BackendSpawnSpec, BackendStatus, HolderEntry, SessionBackend, StreamEnd,
};
use crate::db::Db;
use crate::domain::{
    AttentionState, Session, SessionStatus, SessionSummary, Workspace, WorkspaceInstance,
};
use crate::plugins::usage::TranscriptContext;
use crate::plugins::{attention, AgentContext, AgentPlugin, PluginRegistry};
use crate::util::now_millis;
use crate::workspace;

mod fork;
mod monitor;
mod workspaces;

pub use fork::ForkRequest;

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
    /// Set when this session is a fork: how it inherits the origin's context.
    /// Resolved by [`SessionManager::fork_session`] before the session exists,
    /// and applied here once the working directory is known.
    pub fork: Option<ForkPlan>,
}

/// A resolved decision about how a forked session picks up its origin's context.
/// Built by [`SessionManager::fork_session`], consumed by `create_session`.
#[derive(Debug, Clone)]
pub struct ForkPlan {
    /// Origin session id — persisted as `forked_from`, giving the UI a lineage.
    pub origin_id: String,
    pub seed: ForkSeed,
}

/// The two ways a fork inherits context. Every fork gets exactly one.
#[derive(Debug, Clone)]
pub enum ForkSeed {
    /// The agent reloads the origin's *own* conversation, forking it so the
    /// origin's transcript is never appended to. Full fidelity, no summarizing,
    /// and only possible when the fork keeps the same agent.
    Native { native_id: String },
    /// The agent is pointed at a brief written into its working directory.
    /// The path is handed over, never the text: a transcript typed into a TUI hits
    /// bracketed-paste and input-length limits, and one passed in argv would be
    /// readable by any process on the box via `/proc`.
    Brief { markdown: String },
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
    db: Db,
    registry: Arc<PluginRegistry>,
    backend: Arc<dyn SessionBackend>,
    live: Mutex<HashMap<String, Arc<dyn BackendSession>>>,
    /// Base directory under which per-session Git worktrees are created.
    worktree_root: PathBuf,
    /// Per-session interaction signals, keyed by session id (see [`Interaction`]).
    interactions: Mutex<HashMap<String, Arc<Interaction>>>,
    /// Serializes reconcile passes. Startup runs one in the background while a
    /// holder reconnect can fire another; without this, both could see the same
    /// session as "alive in the holder, not in `live`" and adopt it twice.
    /// Only ever taken on a blocking thread (a pass is a serial holder RPC).
    reconcile_pass: Mutex<()>,
    /// `false` until the startup reconcile pass has finished. See
    /// [`Self::wait_until_ready`].
    ready_tx: watch::Sender<bool>,
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
            reconcile_pass: Mutex::new(()),
            ready_tx: watch::channel(false).0,
        }
    }

    /// The persistence handle. Crate-internal accessor so auth/ws handlers read
    /// through one deliberate door instead of a public field (keeping the DB out
    /// of any external surface). Session logic inside this module uses the field.
    pub(crate) fn db(&self) -> &Db {
        &self.db
    }

    /// The agent plugin registry (read-only, shared). Crate-internal accessor,
    /// same rationale as [`Self::db`].
    pub(crate) fn registry(&self) -> &PluginRegistry {
        &self.registry
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

        let mut ctx = AgentContext {
            command: req.command.clone(),
            extra_args: req.args.clone(),
            extra_env: req.env.clone(),
            options: req.options.clone(),
        };

        // A fork's launch line differs from a fresh session's: either it resumes
        // the origin's conversation natively, or it opens pointed at a brief we
        // write into the working directory we just resolved. Both need the cwd,
        // which is why this happens here and not in `fork_session`.
        let launch = match &req.fork {
            Some(plan) => self.fork_launch(&plugin, &mut ctx, plan, &resolved_cwd)?,
            None => plugin.build_launch(&ctx)?,
        };

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
            // Captured later, by the monitor, while this session is alive.
            agent_session_id: None,
            forked_from: req.fork.as_ref().map(|f| f.origin_id.clone()),
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
                if plugin.tracks_attention() {
                    let _ = self.db.set_attention(
                        &id,
                        AttentionState::Failed,
                        Some("backend spawn failed"),
                        now,
                    );
                }
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

    /// Has the startup reconcile pass finished? Surfaced on `/health` so a
    /// supervisor or gateway can tell "up but still adopting" from "up".
    pub fn is_ready(&self) -> bool {
        *self.ready_tx.borrow()
    }

    /// Resolve once the startup reconcile pass has finished.
    ///
    /// The HTTP listener binds *before* that pass runs, so between bind and
    /// ready a session the previous daemon left running is in the DB as
    /// `running` but not yet in `live`. Any request that resolves a live handle
    /// must wait here first — otherwise attaching to a perfectly healthy session
    /// takes the not-live path and the client is served a dead, read-only
    /// terminal. Callers should bound the wait (see `api::READY_WAIT`).
    pub async fn wait_until_ready(&self) {
        let mut rx = self.ready_tx.subscribe();
        loop {
            if *rx.borrow_and_update() {
                return;
            }
            if rx.changed().await.is_err() {
                return; // sender lives in `self`, so this is unreachable in practice
            }
        }
    }

    /// Run [`Self::startup_reconcile`] off the boot path, marking the manager
    /// ready when it lands.
    ///
    /// Adoption is a serial holder round-trip per session, so it scales with how
    /// many sessions survived: a restart with 7 live sessions spent 8s here,
    /// during which the daemon had not yet bound its port and `start.sh`'s 6s
    /// health check declared a perfectly healthy daemon dead. Reconciling behind
    /// the listener keeps boot-to-`/health` flat regardless of session count.
    pub fn spawn_startup_reconcile(self: &Arc<Self>) {
        let mgr = self.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = mgr.startup_reconcile() {
                tracing::error!("startup reconcile failed: {e:#}");
            }
            // Ready even when the pass failed: it is done, and making every
            // attach wait on a pass that will not come back is strictly worse.
            mgr.ready_tx.send_replace(true);
        });
    }

    /// Reconcile sessions left live in the DB after a restart.
    ///
    /// - In-process backend: the PTYs are gone, so those rows become `failed`.
    /// - Out-of-process holder: adopt-or-reconcile against `holder_list()` —
    ///   alive in the holder → **adopt** (re-attach, mark `running`); a real exit
    ///   record → `exited`/`failed`; absent from the holder → **`indeterminate`**
    ///   (the holder itself died, so no completion record exists).
    ///
    /// Blocking (holder RPCs); [`Self::spawn_startup_reconcile`] is how the
    /// daemon runs it.
    pub fn startup_reconcile(self: &Arc<Self>) -> Result<()> {
        let _pass = self.reconcile_pass.lock();
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
        self.reconcile_from_holder(holder)
    }

    /// Re-run reconciliation after the daemon↔asmux connection dropped and came
    /// back (the supervisor has already re-attached the live sessions). A `list`
    /// after any reconnect catches exits missed while detached
    /// (`asmux-protocol.md` → Liveness). A transient `list` failure here is
    /// *not* treated as "the holder is gone" (unlike startup): the sessions are
    /// almost certainly fine, so we log and wait for the next reconnect.
    ///
    /// Blocking, and serialized against the startup pass — a reconnect during
    /// startup adoption must queue behind it, not race it.
    pub fn reconcile_after_reconnect(self: &Arc<Self>) -> Result<()> {
        let _pass = self.reconcile_pass.lock();
        match self.backend.holder_list() {
            Ok(holder) => self.reconcile_from_holder(holder),
            Err(e) => {
                tracing::warn!("post-reconnect holder list failed ({e:#}); skipping reconcile");
                Ok(())
            }
        }
    }

    /// The shared adopt-or-reconcile decision, run at startup and after every
    /// reconnect. For each session the DB still has live:
    ///
    /// - **alive in the holder, already in `live`** → nothing to do (the
    ///   supervisor re-attached it on reconnect).
    /// - **alive in the holder, not in `live`** → adopt (the startup case).
    /// - **a real exit record** → `exited`/`failed`; if the session was still
    ///   live in `live` (it exited while we were detached), drive the normal exit
    ///   path so the monitor writes its summary and clears it.
    /// - **absent from the holder** → `indeterminate` (no completion record); a
    ///   still-live one has its stream closed so its monitor stops.
    fn reconcile_from_holder(self: &Arc<Self>, holder: Vec<HolderEntry>) -> Result<()> {
        let by_id: HashMap<String, HolderEntry> =
            holder.into_iter().map(|h| (h.id.clone(), h)).collect();

        let mut adopted = 0usize;
        let mut reconciled = 0usize;
        for id in self.db.live_session_ids()? {
            let in_live = self.live.lock().contains_key(&id);
            match by_id.get(&id) {
                Some(entry) if entry.alive => {
                    if in_live {
                        continue; // already running; the supervisor re-attached it
                    }
                    let sess = self.db.get_session(&id)?;
                    let (rows, cols) = sess.as_ref().map(|s| (s.rows, s.cols)).unwrap_or((24, 80));
                    let created_at =
                        sess.as_ref().map(|s| s.created_at).unwrap_or_else(now_millis);
                    let plugin = sess
                        .as_ref()
                        .and_then(|s| self.registry.get(&s.agent_plugin_id));
                    match self.backend.adopt(&id, rows, cols) {
                        Ok(Some(handle)) => {
                            self.live.lock().insert(id.clone(), handle.clone());
                            self.db
                                .update_status(&id, SessionStatus::Running, None, now_millis())?;
                            self.clone().spawn_monitor(
                                id.clone(),
                                handle,
                                created_at,
                                plugin.clone(),
                            );
                            adopted += 1;
                            tracing::info!(session = %id, "adopted live holder session");
                        }
                        Ok(None) | Err(_) => {
                            self.reconcile_indeterminate(&id)?;
                            reconciled += 1;
                        }
                    }
                }
                Some(entry) => {
                    // The holder has a real completion record.
                    if in_live {
                        // Exited while we were detached: drive the normal exit
                        // path so the monitor finalizes (summary + `live` removal).
                        self.backend.end_session_stream(
                            &id,
                            StreamEnd::Exited {
                                code: entry.exit_code,
                                signal: entry.exit_signal,
                            },
                        );
                    } else {
                        let (status, code) = if entry.exit_signal != 0 {
                            (SessionStatus::Failed, None)
                        } else {
                            (SessionStatus::Exited, Some(entry.exit_code))
                        };
                        self.db.update_status(&id, status, code, now_millis())?;
                    }
                    reconciled += 1;
                }
                None => {
                    // Absent from the holder: it died → outcome unknown. Close a
                    // still-live stream so its monitor stops, then mark the row.
                    if in_live {
                        self.backend.end_session_stream(&id, StreamEnd::Vanished);
                        self.live.lock().remove(&id);
                    }
                    self.reconcile_indeterminate(&id)?;
                    reconciled += 1;
                }
            }
        }
        tracing::info!("reconcile: adopted {adopted}, reconciled {reconciled} session(s)");
        Ok(())
    }

    /// Mark a session `indeterminate` with the acmux-style advisory. The
    /// advisory badge only applies to agents whose sessions we classify at
    /// all; for a non-tracking agent (plain shell) the `indeterminate` status
    /// already says the daemon doesn't know, and judging the outcome is the
    /// user's job either way.
    fn reconcile_indeterminate(&self, id: &str) -> Result<()> {
        let now = now_millis();
        self.db
            .update_status(id, SessionStatus::Indeterminate, None, now)?;
        let tracks = self
            .db
            .get_session(id)?
            .and_then(|s| self.registry.get(&s.agent_plugin_id))
            .is_none_or(|p| p.tracks_attention());
        if tracks {
            self.db.set_attention(
                id,
                AttentionState::LikelyBlocked,
                Some("no completion record — the session holder exited while this was running; check the preserved output before assuming it finished"),
                now,
            )?;
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

    /// A fresh, running `MockSession` handle — used by both `create` and the
    /// holder `adopt` path.
    fn mock_session() -> Arc<dyn BackendSession> {
        let (tx, _) = broadcast::channel(16);
        let (status_tx, status_rx) = watch::channel(BackendStatus::Running);
        Arc::new(MockSession {
            tx,
            status_tx,
            status_rx,
            seq: AtomicU64::new(0),
            screen: String::new(),
        })
    }

    /// A `SessionBackend` test double. `Default` is native-like — sessions die on
    /// shutdown and there is no holder. The holder knobs simulate an
    /// out-of-process asmux holder so the startup adopt/reconcile branches can be
    /// driven without a real socket.
    #[derive(Default)]
    struct MockBackend {
        /// Simulate a holder that outlives the daemon (asmux): sessions are left
        /// running on shutdown and offered back through `holder_list`/`adopt`.
        keep_on_shutdown: bool,
        /// Canned `holder_list()` result (only consulted when `keep_on_shutdown`).
        holder: Vec<HolderEntry>,
        /// Make `holder_list()` fail, exercising the "all live → indeterminate" arm.
        holder_list_fails: bool,
        /// Whether `adopt()` yields a live handle (`Some`) or gives up (`None`).
        adopt_ok: bool,
        /// Signalled on entry to `adopt()`, so a test can assert on the state the
        /// daemon serves *while* a pass is still running.
        adopt_entered: Option<tokio::sync::mpsc::UnboundedSender<()>>,
        /// Stand in for the real cost of adoption (a holder round-trip per
        /// session — seconds, for a large set).
        adopt_delay: Duration,
        /// Records `end_session_stream` calls as `(id, "exited"|"vanished")` so the
        /// reconnect-reconcile branches can be asserted without a real holder.
        end_calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl SessionBackend for MockBackend {
        fn id(&self) -> &'static str {
            "mock"
        }
        fn create(&self, _spec: BackendSpawnSpec) -> Result<Arc<dyn BackendSession>> {
            Ok(mock_session())
        }
        fn keep_sessions_on_shutdown(&self) -> bool {
            self.keep_on_shutdown
        }
        fn holder_list(&self) -> Result<Vec<HolderEntry>> {
            if self.holder_list_fails {
                bail!("mock holder_list failure");
            }
            Ok(self.holder.clone())
        }
        fn adopt(
            &self,
            _id: &str,
            _rows: u16,
            _cols: u16,
        ) -> Result<Option<Arc<dyn BackendSession>>> {
            if let Some(tx) = &self.adopt_entered {
                let _ = tx.send(());
            }
            std::thread::sleep(self.adopt_delay);
            Ok(self.adopt_ok.then(mock_session))
        }
        fn end_session_stream(&self, id: &str, outcome: StreamEnd) {
            let kind = match outcome {
                StreamEnd::Exited { .. } => "exited",
                StreamEnd::Vanished => "vanished",
            };
            self.end_calls
                .lock()
                .push((id.to_string(), kind.to_string()));
        }
    }

    fn test_manager() -> (Arc<SessionManager>, PathBuf) {
        test_manager_with(Arc::new(MockBackend::default()))
    }

    fn test_manager_with(backend: Arc<dyn SessionBackend>) -> (Arc<SessionManager>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("asm-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("test.sqlite3")).unwrap();
        let registry = Arc::new(PluginRegistry::with_builtins());
        let manager = Arc::new(SessionManager::new(
            db,
            registry,
            backend,
            dir.join("worktrees"),
        ));
        (manager, dir)
    }

    /// Seed a session row already marked `running` (as if a prior daemon left it
    /// live), so `startup_reconcile` finds it via `live_session_ids()`.
    fn insert_running(db: &Db, id: &str) {
        insert_running_agent(db, id, "shell");
    }

    fn insert_running_agent(db: &Db, id: &str, agent: &str) {
        let now = now_millis();
        db.insert_session(&Session {
            id: id.to_string(),
            agent_plugin_id: agent.into(),
            command: "sh".into(),
            args: vec![],
            env: vec![],
            working_directory: std::env::temp_dir().to_string_lossy().into_owned(),
            workspace_id: None,
            status: SessionStatus::Running,
            rows: 24,
            cols: 80,
            last_event_seq: 0,
            exit_code: None,
            attention_state: AttentionState::None,
            attention_reason: None,
            created_at: now,
            updated_at: now,
            last_activity_at: now,
            risky: false,
            agent_session_id: None,
            forked_from: None,
        })
        .unwrap();
    }

    fn holder_entry(id: &str, alive: bool, exit_code: i32, exit_signal: i32) -> HolderEntry {
        HolderEntry {
            id: id.to_string(),
            alive,
            exit_code,
            exit_signal,
        }
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
            fork: None,
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

    // ---- startup_reconcile branches ----
    //
    // These pin every arm of the adopt/reconcile decision that runs when the
    // daemon restarts with sessions still `running` in the DB. They exist so the
    // planned M4 flip of adopt from ring-replay to exact cold-stitch is guarded
    // *before* it lands (today that flip is untested — RF-M4 #4).

    #[tokio::test]
    async fn native_reconcile_marks_live_sessions_failed() {
        // Default mock is native-like: it does not keep sessions on shutdown, so
        // any row still `running` after a restart is unrecoverable → `failed`.
        let (manager, dir) = test_manager();
        insert_running(&manager.db, "s-native");

        manager.startup_reconcile().unwrap();

        let s = manager.get_session("s-native").unwrap().unwrap();
        assert_eq!(s.status, SessionStatus::Failed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn holder_alive_and_adoptable_is_adopted_running() {
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            holder: vec![holder_entry("s-adopt", true, 0, 0)],
            adopt_ok: true,
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_running(&manager.db, "s-adopt");

        manager.startup_reconcile().unwrap();

        let s = manager.get_session("s-adopt").unwrap().unwrap();
        assert_eq!(s.status, SessionStatus::Running, "adopted → running");
        assert!(
            manager.live_handle("s-adopt").is_some(),
            "adopted handle re-attached into the live map"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn startup_reconcile_runs_behind_the_listener_and_gates_live_lookups() {
        // Adoption is serial holder I/O, so it must not sit on the boot path: the
        // daemon binds, then adopts. The cost of that is a window where a
        // survivor is `running` in the DB but not yet in `live` — and an attach
        // landing there would take the not-live path and serve a dead, read-only
        // terminal for a session that is perfectly alive. So: never block the
        // boot, always block the live lookup.
        let (entered_tx, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            holder: vec![holder_entry("s-slow", true, 0, 0)],
            adopt_ok: true,
            adopt_entered: Some(entered_tx),
            adopt_delay: Duration::from_millis(300),
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_running(&manager.db, "s-slow");

        manager.spawn_startup_reconcile();

        // Mid-pass (the mock is parked inside `adopt`): the daemon is already
        // serving — this is what keeps boot-to-`/health` flat no matter how many
        // sessions survived — but it does not yet claim to be ready.
        entered_rx.recv().await.unwrap();
        assert!(!manager.is_ready(), "the pass must not run on the boot path");
        assert!(manager.live_handle("s-slow").is_none(), "not adopted yet");

        // A live-session request parks here rather than reading that empty map.
        manager.wait_until_ready().await;

        assert!(manager.is_ready());
        assert!(
            manager.live_handle("s-slow").is_some(),
            "once the pass lands, the survivor is live and an attach streams it"
        );
        assert_eq!(
            manager.get_session("s-slow").unwrap().unwrap().status,
            SessionStatus::Running
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn holder_alive_but_unadoptable_is_indeterminate() {
        // Alive in the holder, but adopt() cannot recover it → indeterminate.
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            holder: vec![holder_entry("s-noadopt", true, 0, 0)],
            adopt_ok: false,
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_running(&manager.db, "s-noadopt");

        manager.startup_reconcile().unwrap();

        let s = manager.get_session("s-noadopt").unwrap().unwrap();
        assert_eq!(s.status, SessionStatus::Indeterminate);
        assert!(manager.live_handle("s-noadopt").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn holder_dead_entry_reconciles_to_exit_outcome() {
        // A real completion record from the holder → exited (clean) or failed
        // (signalled), never indeterminate.
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            holder: vec![
                holder_entry("s-exit", false, 7, 0),
                holder_entry("s-signal", false, 0, 9),
            ],
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_running(&manager.db, "s-exit");
        insert_running(&manager.db, "s-signal");

        manager.startup_reconcile().unwrap();

        let exited = manager.get_session("s-exit").unwrap().unwrap();
        assert_eq!(exited.status, SessionStatus::Exited);
        assert_eq!(exited.exit_code, Some(7));

        let signalled = manager.get_session("s-signal").unwrap().unwrap();
        assert_eq!(signalled.status, SessionStatus::Failed, "signalled → failed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn holder_absent_entry_is_indeterminate() {
        // The session is gone from the holder (the holder itself died), so no
        // completion record exists → indeterminate. The advisory badge flags a
        // tracked agent's session for the user; a shell (non-tracking) gets the
        // indeterminate status but never a badge — its outcome is the user's to
        // judge either way.
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_running_agent(&manager.db, "s-absent", "claude");
        insert_running(&manager.db, "s-absent-shell");

        manager.startup_reconcile().unwrap();

        let s = manager.get_session("s-absent").unwrap().unwrap();
        assert_eq!(s.status, SessionStatus::Indeterminate);
        assert_eq!(s.attention_state, AttentionState::LikelyBlocked);

        let sh = manager.get_session("s-absent-shell").unwrap().unwrap();
        assert_eq!(sh.status, SessionStatus::Indeterminate);
        assert_eq!(sh.attention_state, AttentionState::None, "shells carry no badge");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn holder_list_failure_reconciles_all_indeterminate() {
        // If the holder cannot even be listed, every live session is unknown.
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            holder_list_fails: true,
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_running(&manager.db, "s-a");
        insert_running(&manager.db, "s-b");

        manager.startup_reconcile().unwrap();

        for id in ["s-a", "s-b"] {
            assert_eq!(
                manager.get_session(id).unwrap().unwrap().status,
                SessionStatus::Indeterminate
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    // ---- reconcile-after-reconnect branches ----
    //
    // Unlike startup, on a reconnect the live sessions are already in `self.live`
    // (the supervisor re-attached them). `reconcile_from_holder` must leave the
    // survivors alone and only finalize the ones a fresh `list` shows gone.

    /// Put a still-live session into `self.live` (as the supervisor's re-attach
    /// leaves it) and seed a matching running DB row.
    fn insert_live(manager: &Arc<SessionManager>, id: &str) {
        insert_running(&manager.db, id);
        manager.live.lock().insert(id.to_string(), mock_session());
    }

    #[tokio::test]
    async fn reconnect_leaves_live_survivor_running() {
        // Holder still reports it alive AND it's already live → no-op.
        let end_calls = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            adopt_ok: false, // adopt must NOT be called for an already-live session
            end_calls: end_calls.clone(),
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_live(&manager, "s-live");

        manager
            .reconcile_from_holder(vec![holder_entry("s-live", true, 0, 0)])
            .unwrap();

        assert_eq!(
            manager.get_session("s-live").unwrap().unwrap().status,
            SessionStatus::Running
        );
        assert!(manager.live_handle("s-live").is_some());
        assert!(end_calls.lock().is_empty(), "survivor is not finalized");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn reconnect_dead_entry_in_live_drives_exit_path() {
        // Exited while detached: reconcile drives the normal exit path (the
        // monitor writes the summary), so it must call end_session_stream(Exited).
        let end_calls = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            end_calls: end_calls.clone(),
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_live(&manager, "s-exit");

        manager
            .reconcile_from_holder(vec![holder_entry("s-exit", false, 7, 0)])
            .unwrap();

        assert_eq!(
            *end_calls.lock(),
            vec![("s-exit".to_string(), "exited".to_string())]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn reconnect_absent_in_live_is_vanished_indeterminate() {
        // The holder no longer knows it (crash/replace): close the stream and
        // mark the row indeterminate, removing it from the live map.
        let end_calls = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(MockBackend {
            keep_on_shutdown: true,
            end_calls: end_calls.clone(),
            ..Default::default()
        });
        let (manager, dir) = test_manager_with(backend);
        insert_live(&manager, "s-gone");

        manager.reconcile_from_holder(vec![]).unwrap();

        assert_eq!(
            *end_calls.lock(),
            vec![("s-gone".to_string(), "vanished".to_string())]
        );
        assert!(manager.live_handle("s-gone").is_none(), "removed from live");
        assert_eq!(
            manager.get_session("s-gone").unwrap().unwrap().status,
            SessionStatus::Indeterminate
        );
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

    /// Archiving a session that ran on a branch the *user* already had must
    /// reclaim the worktree we made for it and leave the branch standing. This
    /// regressed once: teardown inferred "we own this branch" from the isolation
    /// mode, so a session started on `main` deleted `main` on archive.
    #[tokio::test]
    async fn archive_keeps_a_preexisting_branch_it_only_borrowed() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);

        // `main` exists but is not the repo's checked-out branch, so the session
        // gets a managed worktree with `main` checked out inside it (rather than
        // sharing the source checkout).
        git_in(&repo, &["branch", "main"]);
        git_in(&repo, &["checkout", "-q", "-b", "other"]);
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        let mut req = ws_req(&ws.id);
        req.branch = Some("main".into());
        req.create_branch = false;
        let s = manager.create_session(req).unwrap();
        let inst = manager.get_instance_for_session(&s.id).unwrap().unwrap();
        assert_eq!(inst.branch.as_deref(), Some("main"));
        assert_eq!(inst.isolation, "worktree");
        assert!(inst.owns_worktree, "we created this worktree");
        assert!(!inst.owns_branch, "`main` was the user's, not ours");

        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // Archive succeeds and takes the worktree — but `main` survives.
        let archived = manager.archive_session(&s.id, false).unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
        assert!(!Path::new(&inst.path).exists(), "worktree reclaimed");
        assert!(
            workspace::branch_exists(&repo, "main"),
            "archiving a session must never delete a branch it did not create"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// `force` widens what we may *discard*, never what we *own*. Forcing the
    /// archive of a session on a borrowed branch — even one carrying unmerged
    /// commits, which is what the `-D` path would silently drop — still keeps it.
    #[tokio::test]
    async fn forced_archive_still_keeps_a_borrowed_branch() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);
        git_in(&repo, &["branch", "release"]);
        git_in(&repo, &["checkout", "-q", "-b", "other"]);
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        let mut req = ws_req(&ws.id);
        req.branch = Some("release".into());
        req.create_branch = false;
        let s = manager.create_session(req).unwrap();
        let inst = manager.get_instance_for_session(&s.id).unwrap().unwrap();
        let wt = Path::new(&inst.path);

        // Unmerged commit + uncommitted change: both `force` triggers at once.
        git_in(wt, &["commit", "-q", "--allow-empty", "-m", "shipped"]);
        std::fs::write(wt.join("dirty.txt"), "wip").unwrap();

        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let archived = manager.archive_session(&s.id, true).unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
        assert!(!wt.exists(), "forced archive reclaims our worktree");
        assert!(
            workspace::branch_exists(&repo, "release"),
            "force discards our work, it must not delete the user's branch"
        );
        // The commit made on `release` is still there.
        let log = git_in(&repo, &["log", "-1", "--format=%s", "release"]);
        assert_eq!(log.trim(), "shipped");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A session dropped into a worktree the user created themselves owns
    /// nothing: archiving releases the claim and leaves the directory and its
    /// branch untouched.
    #[tokio::test]
    async fn archive_leaves_a_worktree_the_user_made() {
        let (manager, dir) = test_manager();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init(&repo);

        // The user's own worktree, on their own branch — nothing to do with us.
        let theirs = dir.join("their-worktree");
        git_in(
            &repo,
            &["worktree", "add", "-q", "-b", "release", theirs.to_str().unwrap()],
        );
        let ws = manager
            .register_workspace("repo".into(), repo.to_string_lossy().into_owned())
            .unwrap();

        let mut req = ws_req(&ws.id);
        req.branch = Some("release".into());
        req.create_branch = false;
        let s = manager.create_session(req).unwrap();
        let inst = manager.get_instance_for_session(&s.id).unwrap().unwrap();
        assert_eq!(inst.isolation, "shared");
        assert_eq!(Path::new(&inst.path), theirs.as_path());
        assert!(!inst.owns_worktree && !inst.owns_branch);

        manager.stop_session(&s.id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        manager.archive_session(&s.id, true).unwrap();
        assert!(theirs.is_dir(), "the user's worktree is not ours to remove");
        assert!(workspace::branch_exists(&repo, "release"));

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
    fn shell_output_is_never_classified() {
        // Shells opt out of attention tracking (`tracks_attention` = false): the
        // user drives the terminal themselves, so even output that would read as
        // an approval gate for an agent must stay unclassified. Only the
        // activity timestamp is recorded.
        let (manager, dir) = test_manager();
        let shell = manager.registry.get("shell").unwrap();
        let handle = mock_handle();
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::None);
        manager.on_output(
            "sid", &handle, b"Proceed? (y/n)", Some(&shell), &mut tail, &mut last_write,
            &mut last_attn, &mut false, 0, false,
        );
        assert_eq!(last_attn, AttentionState::None);
        assert!(last_write > 0, "activity must still be recorded");
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

    #[test]
    fn codex_turn_complete_bell_reads_as_activity_via_on_output() {
        // The reported bug, end-to-end: Codex rings the terminal bell when a turn
        // finishes. The plugin opts out of the bell heuristic and classifies from
        // the screen, so a finished turn (no approval menu) must read as activity
        // — settling to idle — even though the byte chunk carried a real bell.
        let (manager, dir) = test_manager();
        let codex = manager.registry.get("codex").unwrap();
        let screen = "\u{25cf} Committed as ee1d352 \u{2014} done.\n\u{2500} Worked for 10m 19s \u{2500}\u{2500}\n\u{203a} ";
        let handle = mock_handle_with_screen(screen);
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Activity);
        manager.on_output(
            "sid", &handle, b"done.\x07", Some(&codex), &mut tail, &mut last_write,
            &mut last_attn, &mut false, 0, false,
        );
        assert_eq!(last_attn, AttentionState::Activity);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_approval_screen_blocks_via_on_output() {
        // The other half: a real Codex approval menu on screen must read as a
        // block, so the session is flagged for the user.
        let (manager, dir) = test_manager();
        let codex = manager.registry.get("codex").unwrap();
        let screen = " Would you like to run the following command?\n \u{203a} 1. Yes, proceed (y)\n   2. No, and tell Codex what to do differently (esc)\n Press enter to confirm or esc to cancel";
        let handle = mock_handle_with_screen(screen);
        let (mut tail, mut last_write, mut last_attn) =
            (String::new(), 0i64, AttentionState::Activity);
        manager.on_output(
            "sid", &handle, b"\x1b[2K", Some(&codex), &mut tail, &mut last_write,
            &mut last_attn, &mut false, 0, false,
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

    // ---- still-working settle (stays working, does not go idle) ----

    #[test]
    fn codex_idle_settle_waiting_on_sub_agent_stays_working() {
        // The reported bug, end-to-end: Codex is blocked in `wait_agent` — nothing
        // to do itself, so the PTY goes quiet and the silence timer fires — but the
        // turn is still in flight. The settle must read the screen and hold the
        // session at Activity instead of calling it idle.
        let (manager, dir) = test_manager();
        let codex = manager.registry.get("codex").unwrap();
        let screen = "\
\u{2022} Waiting for agents\n\
\u{25e6} Working (49s \u{2022} esc to interrupt) \u{b7} 1 background terminal running \u{b7} /ps to view\n\
\u{203a} Run /review on my current changes";
        let handle = mock_handle_with_screen(screen);
        let sig = Interaction::default();
        let mut last_attn = AttentionState::Activity;
        manager.on_idle("sid", &handle, Some(&codex), &mut last_attn, &sig);
        assert_eq!(last_attn, AttentionState::Activity);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_idle_settle_with_background_terminal_stays_working() {
        // The other half: the turn ended, but the background terminal it started
        // is still running — quiet, yet not done, so it must not settle to idle.
        let (manager, dir) = test_manager();
        let codex = manager.registry.get("codex").unwrap();
        let screen = "\
\u{2022} Started it.\n\
\u{2500} Worked for 5m 50s \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
  1 background terminal running \u{b7} /ps to view \u{b7} /stop to close\n\
\u{203a} Run /review on my current changes";
        let handle = mock_handle_with_screen(screen);
        let sig = Interaction::default();
        let mut last_attn = AttentionState::Activity;
        manager.on_idle("sid", &handle, Some(&codex), &mut last_attn, &sig);
        assert_eq!(last_attn, AttentionState::Activity);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_idle_settle_after_finished_turn_reads_as_idle() {
        // The guard on the two above: with nothing in flight and nothing left
        // running, a finished Codex turn must still settle to idle — otherwise the
        // session would never hand back to the user.
        let (manager, dir) = test_manager();
        let codex = manager.registry.get("codex").unwrap();
        let screen = "\
\u{2022} Ran sleep 25\n\
\u{2022} Command completed successfully.\n\
\u{203a} Run /review on my current changes\n\
  gpt-5.6-sol xhigh \u{b7} ~/dev/agent-workspace";
        let handle = mock_handle_with_screen(screen);
        let sig = Interaction::default();
        let mut last_attn = AttentionState::Activity;
        manager.on_idle("sid", &handle, Some(&codex), &mut last_attn, &sig);
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
