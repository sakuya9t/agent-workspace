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
            _ => SessionStatus::Failed,
        }
    }

    /// A session that is no longer producing live output.
    pub fn is_terminal(&self) -> bool {
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
    Activity,
    LikelyBlocked,
    ApprovalNeeded,
    Failed,
}

impl AttentionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionState::None => "none",
            AttentionState::Activity => "activity",
            AttentionState::LikelyBlocked => "likely_blocked",
            AttentionState::ApprovalNeeded => "approval_needed",
            AttentionState::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> AttentionState {
        match s {
            "activity" => AttentionState::Activity,
            "likely_blocked" => AttentionState::LikelyBlocked,
            "approval_needed" => AttentionState::ApprovalNeeded,
            "failed" => AttentionState::Failed,
            _ => AttentionState::None,
        }
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
