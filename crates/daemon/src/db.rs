use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::Connection;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::domain::{
    AttentionState, Device, Session, SessionStatus, SessionSummary, Workspace, WorkspaceInstance,
};

/// Metadata store plus a handle to the high-volume terminal-event writer.
///
/// Metadata operations go through a single WAL connection guarded by a mutex.
/// Terminal output events are written on a dedicated batching thread so PTY
/// throughput never blocks on metadata locks.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    events: EventSink,
}

/// A single terminal output event queued for persistence.
pub struct EventMsg {
    pub session_id: String,
    pub seq: u64,
    pub ts_ms: i64,
    pub stream: u8,
    pub bytes: Vec<u8>,
    /// The asmux ring cursor *after* this chunk. Persisted (per batch) as the
    /// session's `backend_cursor` so a cold-stitch adopt can re-attach the ring
    /// tail exactly where cold history ends. 0 for the native backend.
    pub head_cursor: u64,
}

/// Cloneable handle used by session backends to enqueue terminal events.
#[derive(Clone)]
pub struct EventSink {
    tx: UnboundedSender<EventMsg>,
}

impl EventSink {
    pub fn send(&self, msg: EventMsg) {
        // If the writer thread is gone the session is tearing down; drop quietly.
        let _ = self.tx.send(msg);
    }
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)
            .with_context(|| format!("opening sqlite db at {}", path.display()))?;
        configure(&conn)?;
        migrate(&conn)?;

        // Dedicated writer connection for terminal events (batched).
        let writer_conn = Connection::open(path).context("opening event-writer connection")?;
        configure(&writer_conn)?;
        let (tx, rx) = unbounded_channel::<EventMsg>();
        std::thread::Builder::new()
            .name("asm-event-writer".into())
            .spawn(move || event_writer_loop(writer_conn, rx))
            .context("spawning event writer thread")?;

        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
            events: EventSink { tx },
        })
    }

    pub fn events(&self) -> EventSink {
        self.events.clone()
    }

    // ---- session metadata ----

    pub fn insert_session(&self, s: &Session) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO sessions (
                id, agent_plugin_id, command, args, env, working_directory, workspace_id,
                status, rows, cols, last_event_seq, exit_code, attention_state, attention_reason,
                created_at, updated_at, last_activity_at, risky, agent_session_id, forked_from
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
            rusqlite::params![
                s.id,
                s.agent_plugin_id,
                s.command,
                serde_json::to_string(&s.args)?,
                serde_json::to_string(&s.env)?,
                s.working_directory,
                s.workspace_id,
                s.status.as_str(),
                s.rows,
                s.cols,
                s.last_event_seq as i64,
                s.exit_code,
                s.attention_state.as_str(),
                s.attention_reason,
                s.created_at,
                s.updated_at,
                s.last_activity_at,
                s.risky as i64,
                s.agent_session_id,
                s.forked_from,
            ],
        )?;
        Ok(())
    }

    /// Record the agent's own conversation id for a live session. Written once —
    /// the first successful capture wins, and later ticks must not overwrite it
    /// with a re-derived guess (see [`Session::agent_session_id`]). The
    /// `IS NULL` guard makes that a property of the statement rather than of
    /// every caller.
    pub fn set_agent_session_id(&self, id: &str, native_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let n = conn.execute(
            "UPDATE sessions SET agent_session_id = ?2
             WHERE id = ?1 AND agent_session_id IS NULL",
            rusqlite::params![id, native_id],
        )?;
        Ok(n > 0)
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_plugin_id, command, args, env, working_directory, workspace_id,
                    status, rows, cols, last_event_seq, exit_code, attention_state, attention_reason,
                    created_at, updated_at, last_activity_at, risky, agent_session_id, forked_from
             FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_plugin_id, command, args, env, working_directory, workspace_id,
                    status, rows, cols, last_event_seq, exit_code, attention_state, attention_reason,
                    created_at, updated_at, last_activity_at, risky, agent_session_id, forked_from
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], row_to_session)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn update_status(
        &self,
        id: &str,
        status: SessionStatus,
        exit_code: Option<i32>,
        updated_at: i64,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions SET status = ?2, exit_code = ?3, updated_at = ?4 WHERE id = ?1",
            rusqlite::params![id, status.as_str(), exit_code, updated_at],
        )?;
        Ok(())
    }

    pub fn update_activity(
        &self,
        id: &str,
        last_event_seq: u64,
        last_activity_at: i64,
        attention: AttentionState,
        attention_reason: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions
             SET last_event_seq = ?2, last_activity_at = ?3, updated_at = ?3,
                 attention_state = ?4, attention_reason = ?5
             WHERE id = ?1",
            rusqlite::params![
                id,
                last_event_seq as i64,
                last_activity_at,
                attention.as_str(),
                attention_reason,
            ],
        )?;
        Ok(())
    }

    pub fn set_attention(
        &self,
        id: &str,
        attention: AttentionState,
        reason: Option<&str>,
        updated_at: i64,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions SET attention_state = ?2, attention_reason = ?3, updated_at = ?4 WHERE id = ?1",
            rusqlite::params![id, attention.as_str(), reason, updated_at],
        )?;
        Ok(())
    }

    pub fn set_size(&self, id: &str, rows: u16, cols: u16, updated_at: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions SET rows = ?2, cols = ?3, updated_at = ?4 WHERE id = ?1",
            rusqlite::params![id, rows, cols, updated_at],
        )?;
        Ok(())
    }

    /// Concatenated raw output for a session after `after_seq`. Used for
    /// exited-session history replay (a diagnostic path, not live resume).
    pub fn read_events_after(&self, session_id: &str, after_seq: u64) -> Result<Vec<u8>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT bytes FROM terminal_events WHERE session_id = ?1 AND seq > ?2 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, after_seq as i64], |row| {
            row.get::<_, Vec<u8>>(0)
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.extend_from_slice(&r?);
        }
        Ok(out)
    }

    pub fn insert_summary(&self, s: &SessionSummary) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO session_summaries (
                id, session_id, agent_plugin_id, started_at, ended_at, duration_ms,
                exit_status, terminal_event_start, terminal_event_end
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            rusqlite::params![
                s.id,
                s.session_id,
                s.agent_plugin_id,
                s.started_at,
                s.ended_at,
                s.duration_ms,
                s.exit_status,
                s.terminal_event_start as i64,
                s.terminal_event_end as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, agent_plugin_id, started_at, ended_at, duration_ms,
                    exit_status, terminal_event_start, terminal_event_end
             FROM session_summaries WHERE session_id = ?1 ORDER BY ended_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([session_id], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                session_id: row.get(1)?,
                agent_plugin_id: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                duration_ms: row.get(5)?,
                exit_status: row.get(6)?,
                terminal_event_start: row.get::<_, i64>(7)? as u64,
                terminal_event_end: row.get::<_, i64>(8)? as u64,
            })
        })?;
        rows.next().transpose().map_err(Into::into)
    }

    // ---- workspaces ----

    pub fn insert_workspace(&self, w: &Workspace) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO workspaces (id, name, root_path, is_git, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![w.id, w.name, w.root_path, w.is_git as i64, w.created_at],
        )?;
        Ok(())
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, root_path, is_git, created_at FROM workspaces ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_workspace)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn get_workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, root_path, is_git, created_at FROM workspaces WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], row_to_workspace)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn delete_workspace(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let n = conn.execute(
            "DELETE FROM workspaces WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(n > 0)
    }

    pub fn set_workspace_git(&self, id: &str, is_git: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE workspaces SET is_git = ?2 WHERE id = ?1",
            rusqlite::params![id, is_git as i64],
        )?;
        Ok(())
    }

    // ---- workspace instances ----

    pub fn insert_instance(&self, i: &WorkspaceInstance) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO workspace_instances
                (id, workspace_id, session_id, path, branch, isolation, status, created_at,
                 owns_worktree, owns_branch)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![
                i.id,
                i.workspace_id,
                i.session_id,
                i.path,
                i.branch,
                i.isolation,
                i.status,
                i.created_at,
                i.owns_worktree as i64,
                i.owns_branch as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_instance_for_session(&self, session_id: &str) -> Result<Option<WorkspaceInstance>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_id, session_id, path, branch, isolation, status, created_at,
                    owns_worktree, owns_branch
             FROM workspace_instances WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([session_id], row_to_instance)?;
        rows.next().transpose().map_err(Into::into)
    }

    /// Every active workspace instance for `workspace_id`, newest first. Backs the
    /// workspace branch-overview's session→branch grouping (the sessions table has
    /// no branch column; `workspace_instances.branch` is the sole link). Covered by
    /// `idx_instances_workspace`.
    pub fn list_active_instances_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<WorkspaceInstance>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_id, session_id, path, branch, isolation, status, created_at,
                    owns_worktree, owns_branch
             FROM workspace_instances
             WHERE workspace_id = ?1 AND status = 'active'
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([workspace_id], row_to_instance)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// The `(owns_worktree, owns_branch)` flags recorded for the newest instance
    /// at `path`, or `None` if no session has ever run there. Ownership belongs
    /// to the resource rather than to whoever is currently sharing it, so a
    /// session joining an existing worktree inherits the flags of the instance
    /// that created it — and inherits nothing (`None`) for a worktree we never
    /// made, which is what keeps a user's own checkout safe from archive.
    pub fn instance_ownership_at_path(&self, path: &str) -> Result<Option<(bool, bool)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT owns_worktree, owns_branch FROM workspace_instances
             WHERE path = ?1 ORDER BY created_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([path], |r| {
            Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0))
        })?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn set_instance_status(&self, id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE workspace_instances SET status = ?2 WHERE id = ?1",
            rusqlite::params![id, status],
        )?;
        Ok(())
    }

    /// How many *other* active instances (excluding `exclude_id`) occupy `path`.
    /// A managed worktree shared by several sessions must survive until the last
    /// one leaves, so teardown only reclaims it when this returns 0.
    pub fn count_active_instances_at_path(&self, path: &str, exclude_id: &str) -> Result<usize> {
        let conn = self.conn.lock();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM workspace_instances
             WHERE path = ?1 AND status = 'active' AND id != ?2",
            rusqlite::params![path, exclude_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    // ---- auth: server identity + devices ----

    /// Load the server identity, creating it (with a fresh enrollment token)
    /// on first run. Returns (server_id, enrollment_token).
    pub fn get_or_create_identity(
        &self,
        server_id: &str,
        enrollment_token: &str,
        now: i64,
    ) -> Result<(String, String)> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO server_identity (id, server_id, enrollment_token, created_at)
             VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![server_id, enrollment_token, now],
        )?;
        read_identity(&conn)
    }

    /// (server_id, enrollment_token). Identity is created at startup.
    pub fn identity(&self) -> Result<(String, String)> {
        read_identity(&self.conn.lock())
    }

    pub fn insert_device(&self, d: &Device) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO devices (id, name, token, created_at, last_seen_at, revoked)
             VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![
                d.id,
                d.name,
                d.token,
                d.created_at,
                d.last_seen_at,
                d.revoked as i64,
            ],
        )?;
        Ok(())
    }

    /// Return a non-revoked device by its bearer token, if any.
    pub fn device_by_token(&self, token: &str) -> Result<Option<Device>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, token, created_at, last_seen_at, revoked
             FROM devices WHERE token = ?1 AND revoked = 0",
        )?;
        let mut rows = stmt.query_map([token], row_to_device)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, token, created_at, last_seen_at, revoked
             FROM devices ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_device)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn revoke_device(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let n = conn.execute(
            "UPDATE devices SET revoked = 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(n > 0)
    }

    pub fn touch_device(&self, id: &str, now: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE devices SET last_seen_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now],
        )?;
        Ok(())
    }

    /// The highest terminal-event `seq` persisted for a session (0 if none). A
    /// cold-stitch adopt continues the sequence from here — not the throttled
    /// `last_event_seq`, which can lag and would make new events collide with
    /// already-persisted seqs (dropped by `INSERT OR IGNORE`).
    pub fn max_event_seq(&self, session_id: &str) -> Result<u64> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT COALESCE(MAX(seq), 0) FROM terminal_events WHERE session_id = ?1")?;
        let mut rows = stmt.query_map([session_id], |r| r.get::<_, i64>(0))?;
        match rows.next() {
            Some(r) => Ok(r?.max(0) as u64),
            None => Ok(0),
        }
    }

    /// The persisted `consumed` cursor — the exact end of this session's cold
    /// history (written per batch by the event writer). A cold-stitch adopt seeds
    /// `vt100` from cold history and re-attaches the holder ring
    /// `FromCursor(consumed)` for the un-drained tail.
    pub fn get_backend_cursor(&self, id: &str) -> Result<u64> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT backend_cursor FROM sessions WHERE id = ?1")?;
        let mut rows = stmt.query_map([id], |r| r.get::<_, i64>(0))?;
        match rows.next() {
            Some(r) => Ok(r?.max(0) as u64),
            None => Ok(0),
        }
    }

    /// Ids of sessions currently recorded live (`starting`/`running`) — the
    /// adopt-or-reconcile candidates at startup.
    pub fn live_session_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT id FROM sessions WHERE status IN ('starting','running') ORDER BY created_at ASC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// On startup, any session left in a live state is reconciled: since the
    /// MVP native backend is in-process, a daemon restart means its PTYs are
    /// gone, so those sessions become `failed` (never silently relaunched).
    pub fn reconcile_orphans_on_startup(&self, now: i64) -> Result<usize> {
        let conn = self.conn.lock();
        let n = conn.execute(
            "UPDATE sessions SET status = 'failed', updated_at = ?1,
                 attention_state = 'failed', attention_reason = 'daemon restarted; backend not recovered'
             WHERE status IN ('starting','running')",
            rusqlite::params![now],
        )?;
        Ok(n)
    }
}

fn read_identity(conn: &Connection) -> Result<(String, String)> {
    let mut stmt =
        conn.prepare("SELECT server_id, enrollment_token FROM server_identity WHERE id = 1")?;
    let row = stmt.query_row([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(row)
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let args_json: String = row.get(3)?;
    let env_json: String = row.get(4)?;
    let status_str: String = row.get(7)?;
    let attention_str: String = row.get(12)?;
    Ok(Session {
        id: row.get(0)?,
        agent_plugin_id: row.get(1)?,
        command: row.get(2)?,
        args: serde_json::from_str(&args_json).unwrap_or_default(),
        env: serde_json::from_str(&env_json).unwrap_or_default(),
        working_directory: row.get(5)?,
        workspace_id: row.get(6)?,
        status: SessionStatus::from_str(&status_str),
        rows: row.get(8)?,
        cols: row.get(9)?,
        last_event_seq: row.get::<_, i64>(10)? as u64,
        exit_code: row.get(11)?,
        attention_state: AttentionState::from_str(&attention_str),
        attention_reason: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        last_activity_at: row.get(16)?,
        risky: row.get::<_, i64>(17)? != 0,
        agent_session_id: row.get(18)?,
        forked_from: row.get(19)?,
    })
}

fn row_to_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<Device> {
    Ok(Device {
        id: row.get(0)?,
        name: row.get(1)?,
        token: row.get(2)?,
        created_at: row.get(3)?,
        last_seen_at: row.get(4)?,
        revoked: row.get::<_, i64>(5)? != 0,
    })
}

fn row_to_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        is_git: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
    })
}

fn row_to_instance(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceInstance> {
    Ok(WorkspaceInstance {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        path: row.get(3)?,
        branch: row.get(4)?,
        isolation: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        owns_worktree: row.get::<_, i64>(8)? != 0,
        owns_branch: row.get::<_, i64>(9)? != 0,
    })
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
        tracing::info!("applied schema migration v1");
    }
    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
        conn.pragma_update(None, "user_version", 2)?;
        tracing::info!("applied schema migration v2");
    }
    if version < 3 {
        conn.execute_batch(SCHEMA_V3)?;
        conn.pragma_update(None, "user_version", 3)?;
        tracing::info!("applied schema migration v3");
    }
    if version < 4 {
        conn.execute_batch(SCHEMA_V4)?;
        conn.pragma_update(None, "user_version", 4)?;
        tracing::info!("applied schema migration v4");
    }
    if version < 5 {
        conn.execute_batch(SCHEMA_V5)?;
        conn.pragma_update(None, "user_version", 5)?;
        tracing::info!("applied schema migration v5");
    }
    if version < 6 {
        conn.execute_batch(SCHEMA_V6)?;
        conn.pragma_update(None, "user_version", 6)?;
        tracing::info!("applied schema migration v6");
    }
    if version < 7 {
        conn.execute_batch(SCHEMA_V7)?;
        conn.pragma_update(None, "user_version", 7)?;
        tracing::info!("applied schema migration v7");
    }
    Ok(())
}

const SCHEMA_V1: &str = r#"
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    agent_plugin_id TEXT NOT NULL,
    command TEXT NOT NULL,
    args TEXT NOT NULL,
    env TEXT NOT NULL,
    working_directory TEXT NOT NULL,
    workspace_id TEXT,
    status TEXT NOT NULL,
    rows INTEGER NOT NULL,
    cols INTEGER NOT NULL,
    last_event_seq INTEGER NOT NULL DEFAULT 0,
    exit_code INTEGER,
    attention_state TEXT NOT NULL DEFAULT 'none',
    attention_reason TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_activity_at INTEGER NOT NULL
);

CREATE TABLE terminal_events (
    session_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    ts_ms INTEGER NOT NULL,
    stream INTEGER NOT NULL,
    bytes BLOB NOT NULL,
    PRIMARY KEY (session_id, seq)
);

CREATE TABLE session_summaries (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    agent_plugin_id TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    exit_status TEXT NOT NULL,
    terminal_event_start INTEGER NOT NULL,
    terminal_event_end INTEGER NOT NULL
);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    is_git INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE workspace_instances (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    session_id TEXT,
    path TEXT NOT NULL,
    branch TEXT,
    isolation TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_instances_session ON workspace_instances(session_id);
CREATE INDEX idx_instances_workspace ON workspace_instances(workspace_id);
"#;

const SCHEMA_V3: &str = r#"
CREATE TABLE server_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    server_id TEXT NOT NULL,
    enrollment_token TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0
);
"#;

const SCHEMA_V4: &str = r#"
ALTER TABLE sessions ADD COLUMN risky INTEGER NOT NULL DEFAULT 0;
"#;

// The asmux ring byte-cursor the daemon has drained + persisted up to
// (`consumed`). On adopt-on-restart the daemon seeds vt100 from its cold history
// and re-attaches the holder ring `FromCursor(backend_cursor)`. 0 = nothing
// drained yet (also the value for native/in-process sessions, which never adopt).
const SCHEMA_V5: &str = r#"
ALTER TABLE sessions ADD COLUMN backend_cursor INTEGER NOT NULL DEFAULT 0;
"#;

// Records what a session's instance actually *created*, so archiving can only
// reclaim what we made. Before this, teardown inferred ownership from
// `isolation` and deleted `branch` for any managed worktree — so a session
// started on an existing branch (`main`, `release`) took that branch down with
// it on archive.
//
// The backfill for pre-existing rows errs toward keeping things: a leaked
// worktree or branch is recoverable (and the orphan sweep collects `asm-session/*`
// leftovers anyway), a deleted `main` is not. So only `asm-session/*` branches —
// the ones only we ever create — are backfilled as owned; a branch the user
// named themselves is left unowned even if we did create it.
const SCHEMA_V6: &str = r#"
ALTER TABLE workspace_instances ADD COLUMN owns_worktree INTEGER NOT NULL DEFAULT 0;
ALTER TABLE workspace_instances ADD COLUMN owns_branch INTEGER NOT NULL DEFAULT 0;

UPDATE workspace_instances SET owns_worktree = 1 WHERE isolation = 'worktree';
UPDATE workspace_instances SET owns_branch = 1
    WHERE isolation = 'worktree' AND branch LIKE 'asm-session/%';
"#;

// Session forking. `agent_session_id` is the agent's own conversation id, written
// once while the session is live (see `Session::agent_session_id` for why it is
// not re-derived at fork time); `forked_from` is the origin's session id.
//
// Both are nullable with no backfill: sessions that predate this migration never
// had their native id captured, so a fork of one falls back to the digest brief.
// Backfilling by re-running the transcript heuristic now would be exactly the
// guess the live capture exists to avoid.
const SCHEMA_V7: &str = r#"
ALTER TABLE sessions ADD COLUMN agent_session_id TEXT;
ALTER TABLE sessions ADD COLUMN forked_from TEXT;
"#;

/// Batches terminal events into transactions to keep write amplification low.
fn event_writer_loop(mut conn: Connection, mut rx: UnboundedReceiver<EventMsg>) {
    // Block for the first event, then drain whatever else is queued.
    // The loop ends when all senders have been dropped.
    while let Some(first) = rx.blocking_recv() {
        let mut batch = vec![first];
        while let Ok(m) = rx.try_recv() {
            batch.push(m);
            if batch.len() >= 512 {
                break;
            }
        }
        if let Err(e) = write_batch(&mut conn, &batch) {
            tracing::error!("terminal event batch write failed: {e:#}");
        }
    }
}

fn write_batch(conn: &mut Connection, batch: &[EventMsg]) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO terminal_events (session_id, seq, ts_ms, stream, bytes)
             VALUES (?1,?2,?3,?4,?5)",
        )?;
        for m in batch {
            stmt.execute(rusqlite::params![
                m.session_id,
                m.seq as i64,
                m.ts_ms,
                m.stream,
                m.bytes
            ])?;
        }
    }
    // Advance each session's persisted `backend_cursor` to the max ring cursor in
    // this batch (never regressing), atomically with the events it belongs to.
    // This is the exact end-of-cold-history anchor a cold-stitch adopt re-attaches
    // from. Only holder-backed sessions carry a non-zero cursor.
    {
        let mut max_cursor: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
        for m in batch {
            if m.head_cursor > 0 {
                let e = max_cursor.entry(m.session_id.as_str()).or_insert(0);
                *e = (*e).max(m.head_cursor);
            }
        }
        if !max_cursor.is_empty() {
            let mut stmt = tx.prepare_cached(
                "UPDATE sessions SET backend_cursor = MAX(backend_cursor, ?2) WHERE id = ?1",
            )?;
            for (id, cursor) in max_cursor {
                stmt.execute(rusqlite::params![id, cursor as i64])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AttentionState, Session, SessionStatus};

    fn temp_db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("asm-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (Db::open(&dir.join("t.sqlite3")).unwrap(), dir)
    }

    fn seed_session(db: &Db, id: &str) {
        db.insert_session(&Session {
            id: id.into(),
            agent_plugin_id: "shell".into(),
            command: "sh".into(),
            args: vec![],
            env: vec![],
            working_directory: "/tmp".into(),
            workspace_id: None,
            status: SessionStatus::Running,
            rows: 24,
            cols: 80,
            last_event_seq: 0,
            exit_code: None,
            attention_state: AttentionState::None,
            attention_reason: None,
            created_at: 1,
            updated_at: 1,
            last_activity_at: 1,
            risky: false,
            agent_session_id: None,
            forked_from: None,
        })
        .unwrap();
    }

    /// The exact cold-stitch anchor: after a batch flush, `backend_cursor` equals
    /// the max ring cursor persisted, and `max_event_seq` the max seq — so an
    /// adopt re-attaches from the true end of cold history and continues the
    /// sequence without colliding with existing rows.
    #[test]
    fn backend_cursor_and_max_seq_track_persisted_events() {
        let (db, dir) = temp_db();
        seed_session(&db, "s1");
        let sink = db.events();
        for (seq, cursor) in [(1u64, 100u64), (2, 250), (3, 400)] {
            sink.send(EventMsg {
                session_id: "s1".into(),
                seq,
                ts_ms: 0,
                stream: 0,
                bytes: vec![b'x'; 10],
                head_cursor: cursor,
            });
        }
        // Let the event-writer thread flush the batch.
        std::thread::sleep(std::time::Duration::from_millis(250));

        assert_eq!(db.get_backend_cursor("s1").unwrap(), 400);
        assert_eq!(db.max_event_seq("s1").unwrap(), 3);
        assert_eq!(db.read_events_after("s1", 0).unwrap().len(), 30);
        // A later batch never regresses the cursor.
        sink.send(EventMsg {
            session_id: "s1".into(),
            seq: 4,
            ts_ms: 0,
            stream: 0,
            bytes: vec![b'y'; 5],
            head_cursor: 550,
        });
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert_eq!(db.get_backend_cursor("s1").unwrap(), 550);
        assert_eq!(db.max_event_seq("s1").unwrap(), 4);

        let _ = std::fs::remove_dir_all(dir);
    }
}
