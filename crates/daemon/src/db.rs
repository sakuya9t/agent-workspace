use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::Connection;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::domain::{AttentionState, Session, SessionStatus, SessionSummary};

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
                created_at, updated_at, last_activity_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
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
            ],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_plugin_id, command, args, env, working_directory, workspace_id,
                    status, rows, cols, last_event_seq, exit_code, attention_state, attention_reason,
                    created_at, updated_at, last_activity_at
             FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_plugin_id, command, args, env, working_directory, workspace_id,
                    status, rows, cols, last_event_seq, exit_code, attention_state, attention_reason,
                    created_at, updated_at, last_activity_at
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], row_to_session)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
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
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
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

/// Batches terminal events into transactions to keep write amplification low.
fn event_writer_loop(mut conn: Connection, mut rx: UnboundedReceiver<EventMsg>) {
    loop {
        // Block for the first event, then drain whatever else is queued.
        let first = match rx.blocking_recv() {
            Some(m) => m,
            None => break, // all senders dropped
        };
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
    tx.commit()?;
    Ok(())
}
