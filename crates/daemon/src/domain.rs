use serde::{Deserialize, Serialize};

/// Lifecycle state of a session. Mirrors the architecture doc's session states,
/// trimmed to the ones the MVP engine actually transitions through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Starting,
    Running,
    Exited,
    Failed,
    Stopped,
    Archived,
    /// The session holder (asmux) exited while this session was running, so no
    /// completion record was ever persisted — the outcome is unknown (it may have
    /// finished, been killed, or still be running as an orphan). Distinct from
    /// `failed`, which asserts a real failure. See docs/durable-sessions.md →
    /// Reconciliation states.
    Indeterminate,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Starting => "starting",
            SessionStatus::Running => "running",
            SessionStatus::Exited => "exited",
            SessionStatus::Failed => "failed",
            SessionStatus::Stopped => "stopped",
            SessionStatus::Archived => "archived",
            SessionStatus::Indeterminate => "indeterminate",
        }
    }

    pub fn from_str(s: &str) -> SessionStatus {
        match s {
            "starting" => SessionStatus::Starting,
            "running" => SessionStatus::Running,
            "exited" => SessionStatus::Exited,
            "failed" => SessionStatus::Failed,
            "stopped" => SessionStatus::Stopped,
            "archived" => SessionStatus::Archived,
            "indeterminate" => SessionStatus::Indeterminate,
            _ => SessionStatus::Failed,
        }
    }

    /// A session that is no longer producing live output. `indeterminate` counts:
    /// there is no live backend to attach to, so it behaves terminally (history
    /// only) even though its true outcome is unknown.
    ///
    /// This answers "is there anything to attach to?" — **not** "is it safe to
    /// throw away?". For that, see
    /// [`is_definitively_ended`](Self::is_definitively_ended).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SessionStatus::Exited
                | SessionStatus::Failed
                | SessionStatus::Stopped
                | SessionStatus::Archived
                | SessionStatus::Indeterminate
        )
    }

    /// The session is **known** to be over: the process is gone and will not
    /// resume, so the work it owned (worktree, branch, workspace registration)
    /// is ours to discard.
    ///
    /// `indeterminate` is deliberately excluded, which is the whole reason this
    /// exists apart from [`is_terminal`](Self::is_terminal). An indeterminate
    /// session lost its completion record when the holder died — the process may
    /// have finished, or may still be running as an orphan writing to that very
    /// worktree. Authorizing destruction on "not attachable" would delete a live
    /// agent's branch. The client already draws this line (`isTerminal` in
    /// `client/src/status.ts` excludes `indeterminate`, so the UI offers neither
    /// archive nor cleanup); before this predicate existed, a direct API call
    /// could do exactly what the UI called unsafe.
    ///
    /// RF-LIFE replaces both booleans with named per-transition capabilities;
    /// until then, use this one for anything destructive.
    pub fn is_definitively_ended(&self) -> bool {
        matches!(
            self,
            SessionStatus::Exited
                | SessionStatus::Failed
                | SessionStatus::Stopped
                | SessionStatus::Archived
        )
    }
}

/// Daemon-computed attention signal for the control center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionState {
    None,
    /// Producing output right now (the agent is working).
    Activity,
    /// Output has stopped and nothing is pending — waiting at a ready prompt for
    /// the next input. Not urgent; the next input starts new work.
    Idle,
    LikelyBlocked,
    /// A decision-prompt was detected in recent output — the agent is blocked and
    /// needs input to *proceed* with what it is doing.
    ApprovalNeeded,
    /// The agent stopped because something went wrong mid-turn (e.g. Claude
    /// Code's "API Error: …"). The process is still alive — this is not
    /// [`Failed`](Self::Failed) — but the turn aborted and the session will sit
    /// silent until the user retries, so it must not read as a calm `Idle`.
    Error,
    Failed,
}

impl AttentionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionState::None => "none",
            AttentionState::Activity => "activity",
            AttentionState::Idle => "idle",
            AttentionState::LikelyBlocked => "likely_blocked",
            AttentionState::ApprovalNeeded => "approval_needed",
            AttentionState::Error => "error",
            AttentionState::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> AttentionState {
        match s {
            "activity" => AttentionState::Activity,
            "idle" => AttentionState::Idle,
            "likely_blocked" => AttentionState::LikelyBlocked,
            "approval_needed" => AttentionState::ApprovalNeeded,
            "error" => AttentionState::Error,
            "failed" => AttentionState::Failed,
            _ => AttentionState::None,
        }
    }

    /// The agent is waiting on the user — a blocking prompt, an approval gate,
    /// or a turn that died mid-flight. These are the states the monitor keeps
    /// *sticky* (redraw noise must not demote them) and the client badges as
    /// "needs attention"; the two definitions have to agree, so this is the
    /// server-side twin of the client's `needsAttention` (`client/src/status.ts`).
    pub fn needs_user(self) -> bool {
        matches!(
            self,
            AttentionState::LikelyBlocked | AttentionState::ApprovalNeeded | AttentionState::Error
        )
    }

    /// The agent has a turn in flight: it is producing output right now
    /// (`activity` — "working"), or it is parked on a prompt waiting for the
    /// user to let the turn it already started proceed (`likely_blocked` /
    /// `approval_needed` — both render as "blocked"). Killing the process here
    /// throws away work mid-flight, so [`SessionManager::stop_session`] refuses
    /// without an explicit force.
    ///
    /// **Only working and blocked are protected.** Everything else stops
    /// without ceremony: `idle` (parked at a ready prompt), `none` (silent — no
    /// signal at all, which is every session under a plugin that opts out of
    /// attention tracking, e.g. a plain shell), and `error` / `failed`, whose
    /// turn has already aborted so there is nothing in flight to lose. That
    /// makes this deliberately narrower than [`needs_user`](Self::needs_user),
    /// which counts `error` — needing attention is not the same as being
    /// mid-turn. The client's twin of this predicate is `isBusy`
    /// (`client/src/status.ts`).
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            AttentionState::Activity | AttentionState::LikelyBlocked | AttentionState::ApprovalNeeded
        )
    }
}

/// Persisted session record (subset of the full architecture model for MVP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent_plugin_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub working_directory: String,
    pub workspace_id: Option<String>,
    pub status: SessionStatus,
    pub rows: u16,
    pub cols: u16,
    pub last_event_seq: u64,
    pub exit_code: Option<i32>,
    pub attention_state: AttentionState,
    pub attention_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_activity_at: i64,
    /// Launched with a guardrail-disabling agent flag (e.g. skip-permissions /
    /// bypass-sandbox). Surfaced as a risk badge in the UI.
    pub risky: bool,
    /// The agent's *own* conversation id (Claude's `sessionId`, Codex's rollout
    /// uuid), captured while the session is alive and never re-derived. A fork
    /// that keeps the same agent resumes this conversation natively.
    ///
    /// Captured live rather than at fork time on purpose: the transcript-matching
    /// heuristics in [`crate::plugins::usage`] are "newest file in the directory"
    /// and are fragile enough that two sessions sharing a cwd can collapse onto
    /// one transcript. Reporting the wrong token count is survivable; *resuming
    /// the wrong conversation* is not.
    ///
    /// Kept off the wire: it addresses a conversation on this host and no client
    /// has a use for it. The API exposes the one fact a client *does* need —
    /// `has_agent_conversation` — so the fork dialog can say whether a fork will
    /// carry the whole conversation or a summary of it.
    #[serde(skip_serializing, default)]
    pub agent_session_id: Option<String>,
    /// Origin session id when this session was forked from another. `None` for a
    /// session started from scratch. Gives the UI a lineage, including a chain
    /// when a fork is itself forked.
    pub forked_from: Option<String>,
    /// When the session entered the state it is in now — the timestamp behind the
    /// list's "blocked 8m" / "idle 3m". Distinct from `last_activity_at`, which
    /// is the last time it *printed* anything: a blocked agent redraws its TUI
    /// continuously, so activity says "just now" for a session that has been
    /// waiting on the user for a quarter of an hour. Moved only when the
    /// `status`/`attention_state` pair changes, and deliberately *not* when the
    /// user views the session (see [`crate::db::Db::clear_attention_for_view`]).
    pub state_since: i64,
}

/// An enrolled client device. The `token` is the bearer credential and is
/// never serialized back to clients (see `DeviceInfo`).
#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub token: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
}

/// Public device metadata (no token).
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
}

impl From<&Device> for DeviceInfo {
    fn from(d: &Device) -> Self {
        DeviceInfo {
            id: d.id.clone(),
            name: d.name.clone(),
            created_at: d.created_at,
            last_seen_at: d.last_seen_at,
            revoked: d.revoked,
        }
    }
}

/// A registered source workspace (repo or plain folder). The set of registered
/// workspaces is the allowlist for workspace-scoped sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub is_git: bool,
    pub created_at: i64,
}

/// An isolated working directory assigned to one session. For Git workspaces
/// this is a managed worktree; otherwise it is the source root (direct/plain).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInstance {
    pub id: String,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub path: String,
    pub branch: Option<String>,
    /// "worktree" | "direct" | "plain"
    pub isolation: String,
    /// "active" | "released"
    pub status: String,
    pub created_at: i64,
    /// Whether we created `path` and may remove it when the last session there
    /// is archived. False when the session joined a worktree that already
    /// existed — the user's own checkout is not ours to reclaim.
    #[serde(default)]
    pub owns_worktree: bool,
    /// Whether we created `branch` and may delete it on archive. False when the
    /// session was handed a branch that already existed (`main`, `release`, a
    /// feature branch): we only borrowed it, and archiving must leave it intact.
    #[serde(default)]
    pub owns_branch: bool,
}

/// Structural session summary written on exit / segment boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub session_id: String,
    pub agent_plugin_id: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub exit_status: String,
    pub terminal_event_start: u64,
    pub terminal_event_end: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_status_round_trips() {
        for s in [
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::Exited,
            SessionStatus::Failed,
            SessionStatus::Stopped,
            SessionStatus::Archived,
            SessionStatus::Indeterminate,
        ] {
            assert_eq!(SessionStatus::from_str(s.as_str()), s);
        }
    }

    #[test]
    fn attention_state_round_trips() {
        for a in [
            AttentionState::None,
            AttentionState::Activity,
            AttentionState::Idle,
            AttentionState::LikelyBlocked,
            AttentionState::ApprovalNeeded,
            AttentionState::Error,
            AttentionState::Failed,
        ] {
            assert_eq!(AttentionState::from_str(a.as_str()), a);
        }
    }

    #[test]
    fn terminal_states_are_terminal() {
        assert!(SessionStatus::Exited.is_terminal());
        assert!(SessionStatus::Stopped.is_terminal());
        assert!(!SessionStatus::Running.is_terminal());
        assert!(!SessionStatus::Starting.is_terminal());
    }

    #[test]
    fn indeterminate_is_terminal_but_never_definitively_ended() {
        // The one status where the two predicates must disagree: nothing to
        // attach to, but no proof the process is gone — so history, yes;
        // deleting its worktree and branch, no.
        assert!(SessionStatus::Indeterminate.is_terminal());
        assert!(!SessionStatus::Indeterminate.is_definitively_ended());

        for s in [
            SessionStatus::Exited,
            SessionStatus::Failed,
            SessionStatus::Stopped,
            SessionStatus::Archived,
        ] {
            assert!(s.is_definitively_ended(), "{s:?} is a known outcome");
        }
        assert!(!SessionStatus::Running.is_definitively_ended());
        assert!(!SessionStatus::Starting.is_definitively_ended());
    }

    #[test]
    fn unknown_status_maps_to_failed() {
        assert_eq!(SessionStatus::from_str("bogus"), SessionStatus::Failed);
    }

    #[test]
    fn busy_covers_working_and_blocked_only() {
        assert!(AttentionState::Activity.is_busy());
        assert!(AttentionState::LikelyBlocked.is_busy());
        assert!(AttentionState::ApprovalNeeded.is_busy());
        // Nothing in flight: a ready prompt, silence, or an aborted turn. A
        // silent session (`none` — an untracked plugin, or one that has not been
        // classified yet) counts as idle, not as work worth guarding.
        assert!(!AttentionState::Idle.is_busy());
        assert!(!AttentionState::None.is_busy());
        assert!(!AttentionState::Error.is_busy());
        assert!(!AttentionState::Failed.is_busy());
    }
}
