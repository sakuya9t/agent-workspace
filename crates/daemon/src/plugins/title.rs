//! Best-effort human titles for sessions — the "summary" the session list shows
//! instead of a worktree uuid.
//!
//! No summarizer runs here: each agent's own LLM already names the session, and
//! this module only reads what it wrote. Claude Code appends `ai-title` records
//! to its per-session JSONL (last one wins), Codex keeps an id → `thread_name`
//! index at `~/.codex/session_index.jsonl`, and opencode stores a `title`
//! column in its sqlite db. When the agent hasn't titled the session (yet), the
//! fallback is its first user prompt. All of these are undocumented internals
//! of the CLIs and can shift between versions, so every reader is best-effort:
//! `None` just means "the UI shows workspace/directory naming instead".
//!
//! Titles are cached for [`TITLE_TTL`], because the session list is polled
//! continuously and a cache miss re-reads a transcript that can run to tens of
//! MB. A finished session's title never changes but is still re-read once per
//! TTL while a client polls — same cost envelope as the usage endpoint, which
//! re-reads the same file on every poll of the selected session.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::conversation::{claude_title, strip_reminders};
use super::usage::{
    claude_transcript_path, codex_rollout_path, home_dir, read_head, TranscriptContext, SLACK_MS,
};

const TITLE_TTL: Duration = Duration::from_secs(30);

/// Longest title we synthesize from a first user prompt.
pub(crate) const TITLE_CHARS: usize = 72;

/// (agent id, cwd, started_at) → when computed + what. Both hits and misses are
/// cached: a session with no transcript would otherwise be re-scanned for on
/// every poll tick.
static CACHE: Mutex<Option<HashMap<(String, String, i64), (Instant, Option<String>)>>> =
    Mutex::new(None);

/// Title of a Claude Code session: the last `ai-title` record in its own
/// transcript, else its first user prompt.
pub fn claude_session_title(cx: &TranscriptContext) -> Option<String> {
    cached("claude", cx, || {
        let path = claude_transcript_path(cx)?;
        let text = fs::read_to_string(&path).ok()?;
        claude_title(&text).or_else(|| claude_first_prompt(&text))
    })
}

/// Title of a Codex session: the `thread_name` its rollout id maps to in
/// `~/.codex/session_index.jsonl`, else the rollout's first user prompt.
pub fn codex_session_title(cx: &TranscriptContext) -> Option<String> {
    cached("codex", cx, || {
        let path = codex_rollout_path(cx)?;
        let named = codex_session_id(&read_head(&path, 64 * 1024)?).and_then(|id| {
            let index = home_dir()?.join(".codex").join("session_index.jsonl");
            codex_index_title(&fs::read_to_string(index).ok()?, &id)
        });
        named.or_else(|| codex_first_prompt(&fs::read_to_string(&path).ok()?))
    })
}

/// Title of an opencode session, from the `session` table of its sqlite db.
/// Older opencode versions kept per-session JSON files instead; those installs
/// simply get no title.
pub fn opencode_session_title(cx: &TranscriptContext) -> Option<String> {
    cached("opencode", cx, || {
        let conn = Connection::open_with_flags(
            opencode_db_path()?,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        opencode_title_from(
            &conn,
            &cx.cwd.to_string_lossy(),
            cx.started_at_ms - SLACK_MS,
        )
    })
}

/// TTL cache around a title computation. The lock is not held while computing —
/// a miss does file IO; two racing misses for one key are harmless.
fn cached(
    agent: &str,
    cx: &TranscriptContext,
    compute: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let key = (
        agent.to_string(),
        cx.cwd.to_string_lossy().into_owned(),
        cx.started_at_ms,
    );
    if let Ok(mut guard) = CACHE.lock() {
        if let Some((at, title)) = guard.get_or_insert_with(HashMap::new).get(&key) {
            if at.elapsed() < TITLE_TTL {
                return title.clone();
            }
        }
    }
    let title = compute();
    if let Ok(mut guard) = CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        // Sessions come and go; drop expired entries once the map is big enough
        // to matter.
        if map.len() > 256 {
            map.retain(|_, (at, _)| at.elapsed() < TITLE_TTL);
        }
        map.insert(key, (Instant::now(), title.clone()));
    }
    title
}

/// First non-empty line of a prompt, clipped to [`TITLE_CHARS`] on a word
/// boundary — a title cut mid-word reads like a bug.
pub(crate) fn clip_title(text: &str) -> Option<String> {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    if first.chars().count() <= TITLE_CHARS {
        return Some(first.to_string());
    }
    let clipped: String = first.chars().take(TITLE_CHARS).collect();
    let cut = clipped
        .rsplit_once(' ')
        .map(|(head, _)| head)
        .unwrap_or(&clipped);
    Some(format!("{}…", cut.trim_end_matches(['.', ',', ';', ':'])))
}

/// First real user prompt in a Claude transcript, for sessions Claude hasn't
/// titled yet. Skips subagent sidechains, `isMeta` plumbing, and user records
/// that only carry tool results.
fn claude_first_prompt(text: &str) -> Option<String> {
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] != "user" || v["isSidechain"] == true || v["isMeta"] == true {
            continue;
        }
        let prompt = match &v["message"]["content"] {
            Value::String(s) => strip_reminders(s),
            Value::Array(blocks) => match blocks
                .iter()
                .find_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten())
            {
                Some(t) => strip_reminders(t),
                None => continue,
            },
            _ => continue,
        };
        if let Some(t) = clip_title(&prompt) {
            return Some(t);
        }
    }
    None
}

/// The session uuid from a rollout head (`session_meta` is the first record).
fn codex_session_id(head: &str) -> Option<String> {
    for line in head.lines() {
        if !line.contains("session_meta") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] == "session_meta" {
            return v["payload"]["id"]
                .as_str()
                .or_else(|| v["payload"]["session_id"].as_str())
                .map(|s| s.to_string());
        }
    }
    None
}

/// Codex appends an `{id, thread_name}` record to its index whenever it
/// (re)names a thread, so the last entry for the id wins.
fn codex_index_title(index: &str, id: &str) -> Option<String> {
    let mut name = None;
    for line in index.lines() {
        if !line.contains(id) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["id"] == id {
            if let Some(n) = v["thread_name"].as_str().filter(|n| !n.trim().is_empty()) {
                name = Some(n.to_string());
            }
        }
    }
    name
}

/// First user prompt from a rollout, for sessions missing from the index.
/// Codex wraps harness payloads in tags (`<user_instructions>`,
/// `<environment_context>`); those aren't something the user typed.
fn codex_first_prompt(text: &str) -> Option<String> {
    for line in text.lines() {
        if !line.contains("user_message") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] != "event_msg" || v["payload"]["type"] != "user_message" {
            continue;
        }
        if let Some(m) = v["payload"]["message"].as_str() {
            if m.trim_start().starts_with('<') {
                continue;
            }
            if let Some(t) = clip_title(m) {
                return Some(t);
            }
        }
    }
    None
}

/// opencode's db under its data dir (`$XDG_DATA_HOME` or `~/.local/share`).
fn opencode_db_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".local").join("share")))?;
    let path = base.join("opencode").join("opencode.db");
    path.is_file().then_some(path)
}

/// The newest top-level opencode session created in `cwd` at/after our launch
/// (times in the db are unix ms), mirroring the transcript-matching heuristic
/// the other agents use.
fn opencode_title_from(conn: &Connection, cwd: &str, min_created_ms: i64) -> Option<String> {
    conn.query_row(
        "SELECT title FROM session
         WHERE directory = ?1 AND parent_id IS NULL AND time_created >= ?2
           AND title IS NOT NULL AND trim(title) != ''
         ORDER BY time_updated DESC LIMIT 1",
        rusqlite::params![cwd, min_created_ms],
        |row| row.get(0),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_title_keeps_short_lines_and_breaks_long_ones_on_a_word() {
        assert_eq!(clip_title("  Fix the bug  ").as_deref(), Some("Fix the bug"));
        assert_eq!(clip_title("\n\nsecond line first\nrest"), Some("second line first".into()));
        let long = "alpha beta gamma ".repeat(10);
        let t = clip_title(&long).unwrap();
        assert!(t.chars().count() <= TITLE_CHARS + 1, "too long: {t}");
        assert!(t.ends_with('…'), "{t}");
        assert!(!t.contains("gam…"), "cut mid-word: {t}");
        assert_eq!(clip_title("   \n  "), None);
    }

    #[test]
    fn claude_first_prompt_skips_plumbing_and_strips_reminders() {
        let text = concat!(
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta noise"}}"#,
            "\n",
            r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"subagent"}}"#,
            "\n",
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"out"}]}}"#,
            "\n",
            r#"{"type":"user","message":{"role":"user","content":"<system-reminder>x</system-reminder>Fix the login flow"}}"#,
        );
        assert_eq!(claude_first_prompt(text).as_deref(), Some("Fix the login flow"));
        assert_eq!(claude_first_prompt(r#"{"type":"mode","mode":"normal"}"#), None);
    }

    #[test]
    fn codex_session_id_reads_the_meta_record() {
        let head = concat!(
            r#"{"timestamp":"t","type":"session_meta","payload":{"session_id":"abc-123","id":"abc-123","cwd":"/repo"}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"cwd":"/repo"}}"#,
        );
        assert_eq!(codex_session_id(head).as_deref(), Some("abc-123"));
        assert_eq!(codex_session_id("not json"), None);
    }

    #[test]
    fn codex_index_title_takes_the_last_matching_entry() {
        let index = concat!(
            r#"{"id":"abc-123","thread_name":"First name","updated_at":"t1"}"#,
            "\n",
            r#"{"id":"other","thread_name":"Not ours","updated_at":"t2"}"#,
            "\n",
            r#"{"id":"abc-123","thread_name":"Renamed later","updated_at":"t3"}"#,
        );
        assert_eq!(codex_index_title(index, "abc-123").as_deref(), Some("Renamed later"));
        assert_eq!(codex_index_title(index, "missing"), None);
    }

    #[test]
    fn codex_first_prompt_skips_tagged_harness_payloads() {
        let text = concat!(
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"<environment_context>stuff</environment_context>"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Design a button icon."}}"#,
        );
        assert_eq!(codex_first_prompt(text).as_deref(), Some("Design a button icon."));
    }

    #[test]
    fn opencode_title_matches_directory_and_start_time() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session (
                id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, title TEXT,
                time_created INTEGER, time_updated INTEGER
            );
            INSERT INTO session VALUES ('old', NULL, '/repo', 'Stale run', 100, 900);
            INSERT INTO session VALUES ('ours', NULL, '/repo', 'Fix the flaky test', 1000, 2000);
            INSERT INTO session VALUES ('child', 'ours', '/repo', 'Subtask', 1100, 2100);
            INSERT INTO session VALUES ('blank', NULL, '/repo', '', 1200, 2200);
            INSERT INTO session VALUES ('elsewhere', NULL, '/other', 'Not ours', 1300, 2300);",
        )
        .unwrap();
        assert_eq!(
            opencode_title_from(&conn, "/repo", 950).as_deref(),
            Some("Fix the flaky test")
        );
        assert_eq!(opencode_title_from(&conn, "/nowhere", 0), None);
        // A schema this query doesn't fit (older opencode) degrades to None.
        let empty = Connection::open_in_memory().unwrap();
        assert_eq!(opencode_title_from(&empty, "/repo", 0), None);
    }
}
