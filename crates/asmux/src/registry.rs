//! The session registry: every live session and tombstone, the total-memory
//! cap with tombstone LRU eviction, and `create` idempotency by launch
//! fingerprint.
//!
//! Memory is accounted by *ring capacity* (the deterministic admission unit):
//! a `create` that would breach the cap first evicts oldest tombstones (LRU);
//! only if it is still over budget against live rings alone does it fail with
//! `MEMORY_LIMIT`. See `docs/asmux-protocol.md` → Session lifecycle & tombstones.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::session::{Session, SpawnError, SpawnSpec};

/// Why a `create` was refused (maps to an `Error` code).
#[derive(Debug)]
pub enum CreateError {
    /// The id exists with a different launch fingerprint (`SESSION_EXISTS`).
    Exists,
    /// Would breach the total-memory cap even after tombstone eviction.
    MemoryLimit,
    /// openpty/fork/exec failed (`SPAWN_FAILED`).
    Spawn(String),
}

/// Result of a `create`: either a freshly spawned session or an idempotent hit
/// on an existing one (same id + fingerprint).
pub enum CreateOutcome {
    Created(Arc<Session>),
    Idempotent(Arc<Session>),
}

/// Result of a `purge`.
#[derive(Debug, PartialEq, Eq)]
pub enum PurgeOutcome {
    Purged,
    /// No session with that id (incl. an already-evicted tombstone).
    Unknown,
    /// The session is still alive; the caller must `kill` first.
    Alive,
}

struct Inner {
    sessions: HashMap<String, Arc<Session>>,
    /// Sum of ring capacities across all sessions (live + tombstone).
    used_bytes: u64,
    /// Monotonic access clock; last-access tick per session id drives LRU.
    tick: u64,
    access: HashMap<String, u64>,
}

pub struct Registry {
    inner: Mutex<Inner>,
    pub instance_id: String,
    pub started_at_unix_ms: i64,
    pub memory_limit: u64,
}

impl Registry {
    pub fn new(instance_id: String, started_at_unix_ms: i64, memory_limit: u64) -> Self {
        Registry {
            inner: Mutex::new(Inner {
                sessions: HashMap::new(),
                used_bytes: 0,
                tick: 0,
                access: HashMap::new(),
            }),
            instance_id,
            started_at_unix_ms,
            memory_limit,
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        let mut inner = self.inner.lock();
        let s = inner.sessions.get(id).cloned();
        if s.is_some() {
            touch(&mut inner, id);
        }
        s
    }

    pub fn list(&self) -> Vec<Arc<Session>> {
        self.inner.lock().sessions.values().cloned().collect()
    }

    pub fn session_count(&self) -> u32 {
        self.inner.lock().sessions.len() as u32
    }

    /// Create (or idempotently return) a session. `spec.ring_capacity` is
    /// assumed already range-checked by the caller.
    pub fn create(&self, spec: SpawnSpec) -> Result<CreateOutcome, CreateError> {
        let mut inner = self.inner.lock();

        // Idempotency: a caller-supplied id that already exists.
        if let Some(existing) = inner.sessions.get(&spec.session_id).cloned() {
            if existing.fingerprint == spec.fingerprint {
                touch(&mut inner, &spec.session_id);
                return Ok(CreateOutcome::Idempotent(existing));
            }
            return Err(CreateError::Exists);
        }

        // Admission control against the total-memory cap.
        let need = spec.ring_capacity as u64;
        if !self.admit(&mut inner, need) {
            return Err(CreateError::MemoryLimit);
        }

        // Reserve, then spawn while still holding the lock (openpty/fork is fast
        // and this keeps the id reservation race-free).
        inner.used_bytes = inner.used_bytes.saturating_add(need);
        let id = spec.session_id.clone();
        match Session::spawn(spec) {
            Ok(session) => {
                inner.sessions.insert(id.clone(), session.clone());
                touch(&mut inner, &id);
                Ok(CreateOutcome::Created(session))
            }
            Err(SpawnError::Spawn(msg)) => {
                inner.used_bytes = inner.used_bytes.saturating_sub(need);
                Err(CreateError::Spawn(msg))
            }
        }
    }

    /// Purge a tombstone: free its ring and drop the record.
    pub fn purge(&self, id: &str) -> PurgeOutcome {
        let mut inner = self.inner.lock();
        match inner.sessions.get(id) {
            None => PurgeOutcome::Unknown,
            Some(s) if s.is_alive() => PurgeOutcome::Alive,
            Some(_) => {
                remove(&mut inner, id);
                PurgeOutcome::Purged
            }
        }
    }

    /// Try to make room for `need` bytes by evicting tombstones LRU. Returns
    /// whether the budget now fits `need`.
    fn admit(&self, inner: &mut Inner, need: u64) -> bool {
        if inner.used_bytes.saturating_add(need) <= self.memory_limit {
            return true;
        }
        // Collect tombstone ids ordered by ascending last-access (LRU first).
        let mut dead: Vec<(u64, String)> = inner
            .sessions
            .iter()
            .filter(|(_, s)| !s.is_alive())
            .map(|(id, _)| {
                let t = inner.access.get(id).copied().unwrap_or(0);
                (t, id.clone())
            })
            .collect();
        dead.sort_by_key(|(t, _)| *t);

        for (_, id) in dead {
            if inner.used_bytes.saturating_add(need) <= self.memory_limit {
                break;
            }
            remove(inner, &id);
        }
        inner.used_bytes.saturating_add(need) <= self.memory_limit
    }
}

/// Move `id` to the most-recently-used position.
fn touch(inner: &mut Inner, id: &str) {
    inner.tick = inner.tick.saturating_add(1);
    let t = inner.tick;
    inner.access.insert(id.to_string(), t);
}

/// Remove a session from the registry and refund its capacity.
fn remove(inner: &mut Inner, id: &str) {
    if let Some(s) = inner.sessions.remove(id) {
        inner.used_bytes = inner.used_bytes.saturating_sub(s.ring_capacity());
    }
    inner.access.remove(id);
}

/// Immutable launch fingerprint: a stable hash over command + args + cwd + env.
/// Env values are hashed, never stored/compared in the clear, so secrets in the
/// environment never leak. Env pairs are sorted so ordering is irrelevant.
pub fn launch_fingerprint(
    command: &str,
    args: &[String],
    cwd: &str,
    env: &[(String, String)],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    command.hash(&mut hasher);
    args.hash(&mut hasher);
    cwd.hash(&mut hasher);
    let mut env_sorted: Vec<&(String, String)> = env.iter().collect();
    env_sorted.sort();
    for (k, v) in env_sorted {
        k.hash(&mut hasher);
        v.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    fn spec(id: &str, cap: usize, fp: u64) -> SpawnSpec {
        SpawnSpec {
            session_id: id.to_string(),
            command: "true".to_string(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
            cols: 80,
            rows: 24,
            ring_capacity: cap,
            metadata: vec![],
            fingerprint: fp,
            created_at_unix_ms: 0,
        }
    }

    #[test]
    fn fingerprint_is_stable_and_env_order_independent() {
        let a = launch_fingerprint(
            "sh",
            &["-c".into(), "x".into()],
            "/tmp",
            &[("A".into(), "1".into()), ("B".into(), "2".into())],
        );
        let b = launch_fingerprint(
            "sh",
            &["-c".into(), "x".into()],
            "/tmp",
            &[("B".into(), "2".into()), ("A".into(), "1".into())],
        );
        assert_eq!(a, b);
        let c = launch_fingerprint("sh", &["-c".into(), "y".into()], "/tmp", &[]);
        assert_ne!(a, c);
    }

    #[test]
    fn idempotent_create_matches_on_fingerprint() {
        let reg = Registry::new("iid".into(), 0, 1024 * 1024);
        let first = reg.create(spec("s1", 16 * 1024, 42)).unwrap();
        let created = matches!(first, CreateOutcome::Created(_));
        assert!(created);
        // Same id + fingerprint => idempotent hit, no second spawn.
        let again = reg.create(spec("s1", 16 * 1024, 42)).unwrap();
        assert!(matches!(again, CreateOutcome::Idempotent(_)));
        // Same id, different fingerprint => SESSION_EXISTS.
        let mismatch = reg.create(spec("s1", 16 * 1024, 99));
        assert!(matches!(mismatch, Err(CreateError::Exists)));
        assert_eq!(reg.session_count(), 1);
    }
}
