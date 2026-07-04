//! Best-effort per-session usage reporting.
//!
//! Agents like Claude Code and Codex persist their own per-session transcripts
//! on the host (the same data their `/status` / `/usage` commands surface). The
//! daemon runs on that host, so it can read those files directly and normalize
//! them into [`AgentUsage`] for the client — no TUI scraping, no API proxy.
//!
//! The mapping from an asm session to the agent's own session file is a
//! heuristic (newest transcript under the matching location, written at/after
//! the session started), so callers should treat the result as best-effort.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

/// Allow the agent's file mtime to predate our recorded start by this much
/// (clock skew / launch lag) before we consider it too stale to be ours.
const SLACK_MS: i64 = 120_000;

/// Inputs a plugin needs to locate its on-disk session transcript.
pub struct UsageContext {
    /// Working directory the session was launched in.
    pub cwd: PathBuf,
    /// Session `created_at` in unix milliseconds.
    pub started_at_ms: i64,
}

/// Normalized usage snapshot for one session, shaped for the client.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentUsage {
    /// Whether usage data was actually found.
    pub available: bool,
    /// Where the numbers came from (transcript path), for the UI + debugging.
    pub source: Option<String>,
    /// Model id as recorded by the agent.
    pub model: Option<String>,
    /// Context-window occupancy for the most recent turn (tokens).
    pub context_tokens: Option<u64>,
    /// Model context-window size (real for Codex, estimated for Claude).
    pub context_window: Option<u64>,
    /// Cumulative fresh (non-cached) input tokens for the session.
    pub input_tokens: Option<u64>,
    /// Cumulative cached / cache-read input tokens for the session.
    pub cached_input_tokens: Option<u64>,
    /// Cumulative output tokens for the session.
    pub output_tokens: Option<u64>,
    /// Cumulative reasoning output tokens (Codex reports these separately).
    pub reasoning_tokens: Option<u64>,
    /// Cumulative total tokens for the session.
    pub total_tokens: Option<u64>,
    /// Subscription rate-limit windows (Codex records these; Claude does not).
    pub rate_limits: Vec<RateLimitWindow>,
    /// ISO timestamp of the reading we parsed.
    pub updated_at: Option<String>,
    /// Human note about method / caveats.
    pub note: Option<String>,
}

/// One rate-limit window as reported by the agent.
#[derive(Debug, Clone, Serialize)]
pub struct RateLimitWindow {
    /// Human label, e.g. "5-hour" or "weekly".
    pub label: String,
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    /// Unix seconds at which the window resets.
    pub resets_at: Option<i64>,
}

// ---------- Claude Code ----------

/// Usage for a Claude Code session: per-session tokens from the on-disk
/// transcript, plus account-wide 5-hour/weekly rate-limit windows from the
/// OAuth usage endpoint (the same data `/usage` shows). Rate limits are still
/// returned when the transcript is missing, so a session whose agent hasn't
/// written one yet isn't a blank modal.
pub fn claude_usage(cx: &UsageContext) -> Option<AgentUsage> {
    let limits = claude_rate_limits();
    match claude_transcript_usage(cx) {
        Some(mut u) => {
            if !limits.is_empty() {
                u.note = Some(format!(
                    "{} Rate limits are account-wide.",
                    u.note.unwrap_or_default()
                ));
            }
            u.rate_limits = limits;
            Some(u)
        }
        None if !limits.is_empty() => Some(AgentUsage {
            available: true,
            rate_limits: limits,
            note: Some(
                "No transcript found for this session yet; showing account-wide Claude rate \
                 limits only."
                    .into(),
            ),
            ..Default::default()
        }),
        None => None,
    }
}

/// Read per-session token usage from `~/.claude/projects/<cwd>/*.jsonl`.
fn claude_transcript_usage(cx: &UsageContext) -> Option<AgentUsage> {
    let dir = home_dir()?
        .join(".claude")
        .join("projects")
        .join(encode_claude_dir(&cx.cwd));
    if !dir.is_dir() {
        return None;
    }
    let file = newest_jsonl_in(&dir, cx.started_at_ms - SLACK_MS)?;
    let text = fs::read_to_string(&file).ok()?;
    let mut u = parse_claude_text(&text)?;
    u.source = Some(format!("Claude transcript {}", file.display()));
    Some(u)
}

/// Claude encodes a project directory by replacing path punctuation with `-`.
fn encode_claude_dir(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| match c {
            '/' | '\\' | '.' | ':' => '-',
            other => other,
        })
        .collect()
}

fn parse_claude_text(text: &str) -> Option<AgentUsage> {
    let mut u = AgentUsage::default();
    let (mut in_sum, mut cr_sum, mut out_sum) = (0u64, 0u64, 0u64);
    let (mut last_in, mut last_cr, mut last_cc) = (0u64, 0u64, 0u64);
    let mut found = false;

    for line in text.lines() {
        if !line.contains("\"usage\"") {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let usage = &v["message"]["usage"];
        if !usage.is_object() {
            continue;
        }
        let ii = usage["input_tokens"].as_u64().unwrap_or(0);
        let cr = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
        let cc = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
        let oo = usage["output_tokens"].as_u64().unwrap_or(0);
        in_sum += ii;
        cr_sum += cr;
        out_sum += oo;
        last_in = ii;
        last_cr = cr;
        last_cc = cc;
        if let Some(m) = v["message"]["model"].as_str() {
            if !m.is_empty() && m != "<synthetic>" {
                u.model = Some(m.to_string());
            }
        }
        if let Some(ts) = v["timestamp"].as_str() {
            u.updated_at = Some(ts.to_string());
        }
        found = true;
    }

    if !found {
        return None;
    }
    u.available = true;
    u.context_tokens = Some(last_in + last_cr + last_cc);
    u.context_window = u.model.as_deref().map(claude_context_window);
    u.input_tokens = Some(in_sum);
    u.cached_input_tokens = Some(cr_sum);
    u.output_tokens = Some(out_sum);
    // Deliberately no summed `total`: cumulative cache-reads dwarf everything and
    // a naive sum is misleading. These per-category cumulatives match `/cost`.
    u.note = Some(
        "Cumulative tokens read from the on-disk Claude transcript (matches /cost); the context \
         window is an estimate for the model."
            .into(),
    );
    Some(u)
}

/// Best-effort context window for a Claude model id (the transcript does not
/// record it). 1M-context variants are tagged; everything else assumes 200k.
fn claude_context_window(model: &str) -> u64 {
    if model.to_lowercase().contains("1m") {
        1_000_000
    } else {
        200_000
    }
}

// ---------- Claude rate limits (account-wide) ----------

/// The OAuth usage endpoint behind Claude Code's `/usage` command. The
/// transcript never records rate-limit windows, so this is the only source.
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The endpoint is account-wide, so one short-lived reading serves every
/// session and 5s UI poll tick without hammering the API.
const CLAUDE_RL_CACHE_TTL: Duration = Duration::from_secs(30);

static CLAUDE_RL_CACHE: Mutex<Option<(Instant, Vec<RateLimitWindow>)>> = Mutex::new(None);

/// Account-wide 5-hour/weekly windows for the Claude subscription, fetched
/// with the CLI's own on-disk OAuth token. Best-effort: empty when there are
/// no credentials (API-key auth, macOS keychain), the token is expired, or the
/// endpoint is unreachable — misses are cached too.
fn claude_rate_limits() -> Vec<RateLimitWindow> {
    if let Ok(guard) = CLAUDE_RL_CACHE.lock() {
        if let Some((at, cached)) = guard.as_ref() {
            if at.elapsed() < CLAUDE_RL_CACHE_TTL {
                return cached.clone();
            }
        }
    }
    let fetched = fetch_claude_rate_limits().unwrap_or_default();
    if let Ok(mut guard) = CLAUDE_RL_CACHE.lock() {
        *guard = Some((Instant::now(), fetched.clone()));
    }
    fetched
}

fn fetch_claude_rate_limits() -> Option<Vec<RateLimitWindow>> {
    let creds_path = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(|d| PathBuf::from(d).join(".credentials.json"))
        .or_else(|| home_dir().map(|h| h.join(".claude").join(".credentials.json")))?;
    let creds: Value = serde_json::from_str(&fs::read_to_string(creds_path).ok()?).ok()?;
    let oauth = &creds["claudeAiOauth"];
    let token = oauth["accessToken"].as_str()?;
    if token.is_empty() {
        return None;
    }
    // An expired access token would only earn us a 401; the CLI refreshes it
    // on its own, so just wait for the next cache miss.
    if let Some(exp_ms) = oauth["expiresAt"].as_i64() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;
        if exp_ms <= now_ms {
            return None;
        }
    }
    let body: Value = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .build()
        .get(CLAUDE_USAGE_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .call()
        .ok()?
        .into_json()
        .ok()?;
    Some(parse_claude_rate_limits(&body))
}

/// Map the OAuth usage payload to normalized rows. Newer payloads carry a
/// `limits` array that includes model-scoped weekly windows (what `/usage`
/// renders); prefer it, falling back to the older `five_hour`/`seven_day`
/// top-level objects. Windows the account doesn't have come back null.
fn parse_claude_rate_limits(v: &Value) -> Vec<RateLimitWindow> {
    fn reset_secs(w: &Value) -> Option<i64> {
        w["resets_at"]
            .as_i64()
            .or_else(|| w["resets_at"].as_str().and_then(iso8601_to_unix_secs))
    }

    let mut out = Vec::new();
    for l in v["limits"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let Some(pct) = l["percent"].as_f64() else {
            continue;
        };
        let (label, minutes) = match l["kind"].as_str() {
            Some("session") => ("5-hour".to_string(), 300),
            Some("weekly_all") => ("weekly".to_string(), 10_080),
            Some("weekly_scoped") => {
                let model = l["scope"]["model"]["display_name"]
                    .as_str()
                    .unwrap_or("scoped");
                (format!("weekly ({model})"), 10_080)
            }
            _ => continue,
        };
        out.push(RateLimitWindow {
            label,
            used_percent: pct,
            window_minutes: Some(minutes),
            resets_at: reset_secs(l),
        });
    }
    if !out.is_empty() {
        return out;
    }

    const WINDOWS: &[(&str, &str, u64)] = &[
        ("five_hour", "5-hour", 300),
        ("seven_day", "weekly", 10_080),
        ("seven_day_opus", "weekly (Opus)", 10_080),
        ("seven_day_sonnet", "weekly (Sonnet)", 10_080),
    ];
    for (key, label, minutes) in WINDOWS {
        let w = &v[*key];
        let Some(pct) = w["utilization"].as_f64() else {
            continue;
        };
        out.push(RateLimitWindow {
            label: (*label).to_string(),
            used_percent: pct,
            window_minutes: Some(*minutes),
            resets_at: reset_secs(w),
        });
    }
    out
}

/// Parse an RFC 3339 timestamp (`2026-07-03T04:00:00Z`, optional fractional
/// seconds, `Z` or numeric offset) into unix seconds, without a date crate.
fn iso8601_to_unix_secs(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |r: std::ops::Range<usize>| -> Option<i64> { s.get(r)?.parse::<i64>().ok() };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Skip fractional seconds, then read the offset.
    let mut rest = s.get(19..)?;
    if let Some(frac) = rest.strip_prefix('.') {
        let end = frac
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(frac.len());
        rest = frac.get(end..)?;
    }
    let off_secs = match rest.as_bytes().first() {
        None | Some(b'Z') | Some(b'z') => 0,
        Some(sign @ (b'+' | b'-')) => {
            let oh: i64 = rest.get(1..3)?.parse().ok()?;
            let om: i64 = match rest.as_bytes().get(3) {
                Some(b':') => rest.get(4..6)?.parse().ok()?,
                Some(_) => rest.get(3..5)?.parse().ok()?,
                None => 0,
            };
            let mag = oh * 3600 + om * 60;
            if *sign == b'-' {
                -mag
            } else {
                mag
            }
        }
        _ => return None,
    };
    // Days since the epoch for a civil date (Howard Hinnant's algorithm).
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + sec - off_secs)
}

// ---------- Codex ----------

/// Read usage for a Codex session from `~/.codex/sessions/**/rollout-*.jsonl`.
pub fn codex_usage(cx: &UsageContext) -> Option<AgentUsage> {
    let root = home_dir()?.join(".codex").join("sessions");
    if !root.is_dir() {
        return None;
    }
    let mut files = Vec::new();
    collect_jsonl(&root, &mut files, 0);
    let min = cx.started_at_ms - SLACK_MS;
    files.retain(|(m, _)| *m >= min);
    files.sort_by_key(|(m, _)| std::cmp::Reverse(*m)); // newest first

    // Prefer the rollout whose recorded cwd matches this session; otherwise take
    // the most recently written candidate.
    let want = cx.cwd.to_string_lossy().to_string();
    let chosen = files
        .iter()
        .take(20)
        .find(|(_, p)| read_head(p, 64 * 1024).and_then(|h| codex_file_cwd(&h)).as_deref() == Some(want.as_str()))
        .or_else(|| files.first())
        .map(|(_, p)| p.clone())?;

    let text = fs::read_to_string(&chosen).ok()?;
    let mut u = parse_codex_text(&text)?;
    u.source = Some(format!("Codex rollout {}", chosen.display()));
    Some(u)
}

fn parse_codex_text(text: &str) -> Option<AgentUsage> {
    let mut last: Option<Value> = None;
    let mut model: Option<String> = None;

    for line in text.lines() {
        let has_model = line.contains("\"model\"");
        let has_tc = line.contains("token_count");
        if !has_model && !has_tc {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if has_model {
            if let Some(m) = v["payload"]["model"].as_str().or_else(|| v["model"].as_str()) {
                if !m.is_empty() {
                    model = Some(m.to_string());
                }
            }
        }
        if has_tc && v["payload"]["type"] == "token_count" {
            last = Some(v);
        }
    }

    let v = last?;
    let info = &v["payload"]["info"];
    let total = &info["total_token_usage"];
    let last_turn = &info["last_token_usage"];

    let mut u = AgentUsage {
        available: true,
        model,
        context_window: info["model_context_window"].as_u64(),
        context_tokens: last_turn["input_tokens"]
            .as_u64()
            .or_else(|| last_turn["total_tokens"].as_u64()),
        input_tokens: total["input_tokens"].as_u64(),
        cached_input_tokens: total["cached_input_tokens"].as_u64(),
        output_tokens: total["output_tokens"].as_u64(),
        reasoning_tokens: total["reasoning_output_tokens"].as_u64(),
        total_tokens: total["total_tokens"].as_u64(),
        updated_at: v["timestamp"].as_str().map(|s| s.to_string()),
        ..Default::default()
    };

    let limits = &v["payload"]["rate_limits"];
    for key in ["primary", "secondary"] {
        let w = &limits[key];
        if let Some(pct) = w["used_percent"].as_f64() {
            let window_minutes = w["window_minutes"].as_u64();
            u.rate_limits.push(RateLimitWindow {
                label: window_minutes.map(window_label).unwrap_or_else(|| key.to_string()),
                used_percent: pct,
                window_minutes,
                resets_at: w["resets_at"].as_i64(),
            });
        }
    }
    u.note = Some("Read from the on-disk Codex session rollout.".into());
    Some(u)
}

/// Extract the recorded cwd from the head of a Codex rollout (session_meta /
/// turn_context records carry it).
fn codex_file_cwd(head: &str) -> Option<String> {
    for line in head.lines() {
        if !line.contains("\"cwd\"") {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(c) = v["payload"]["cwd"].as_str().or_else(|| v["cwd"].as_str()) {
                return Some(c.to_string());
            }
        }
    }
    None
}

/// Human label for a rate-limit window given its length in minutes.
fn window_label(minutes: u64) -> String {
    if minutes == 0 {
        "window".to_string()
    } else if minutes.is_multiple_of(1440) {
        let d = minutes / 1440;
        if d == 7 {
            "weekly".to_string()
        } else {
            format!("{d}-day")
        }
    } else if minutes.is_multiple_of(60) {
        format!("{}-hour", minutes / 60)
    } else {
        format!("{minutes}-min")
    }
}

// ---------- shared fs helpers ----------

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Newest `*.jsonl` in `dir` whose mtime is at least `min_mtime_ms`.
fn newest_jsonl_in(dir: &Path, min_mtime_ms: i64) -> Option<PathBuf> {
    let mut best: Option<(i64, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let m = entry.metadata().ok().map(|md| mtime_ms(&md)).unwrap_or(0);
        if m < min_mtime_ms {
            continue;
        }
        if best.as_ref().map(|(bm, _)| m > *bm).unwrap_or(true) {
            best = Some((m, path));
        }
    }
    best.map(|(_, p)| p)
}

/// Recursively collect `(mtime_ms, path)` for `*.jsonl` files under `dir`.
fn collect_jsonl(dir: &Path, out: &mut Vec<(i64, PathBuf)>, depth: u32) {
    if depth > 6 {
        return;
    }
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let md = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.is_dir() {
            collect_jsonl(&path, out, depth + 1);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push((mtime_ms(&md), path));
        }
    }
}

/// Read up to `max` bytes from the start of a file (avoids loading huge rollouts
/// just to sniff their cwd).
fn read_head(path: &Path, max: usize) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut buf = vec![0u8; max];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_transcript() {
        let text = concat!(
            r#"{"type":"operation","timestamp":"2026-07-02T10:00:00Z","sessionId":"x"}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-07-02T10:00:01Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":50}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-07-02T10:00:05Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":2,"cache_creation_input_tokens":200,"cache_read_input_tokens":2000,"output_tokens":80}}}"#,
        );
        let u = parse_claude_text(text).expect("usage");
        assert!(u.available);
        assert_eq!(u.model.as_deref(), Some("claude-opus-4-8"));
        // Last turn context = 2 + 200 + 2000.
        assert_eq!(u.context_tokens, Some(2202));
        assert_eq!(u.context_window, Some(200_000));
        // Cumulative output = 50 + 80.
        assert_eq!(u.output_tokens, Some(130));
        assert_eq!(u.updated_at.as_deref(), Some("2026-07-02T10:00:05Z"));
    }

    #[test]
    fn parses_codex_rollout() {
        let text = concat!(
            r#"{"type":"turn_context","timestamp":"2026-07-02T10:00:00Z","payload":{"cwd":"/home/x/proj","model":"gpt-5-codex"}}"#,
            "\n",
            r#"{"type":"event_msg","timestamp":"2026-07-02T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":12980,"cached_input_tokens":4992,"output_tokens":3389,"reasoning_output_tokens":1552,"total_tokens":16369},"last_token_usage":{"input_tokens":9000,"total_tokens":10000},"model_context_window":258400},"rate_limits":{"primary":{"used_percent":1.0,"window_minutes":300,"resets_at":1782948446},"secondary":{"used_percent":12.5,"window_minutes":10080,"resets_at":1783000000}}}}"#,
        );
        let u = parse_codex_text(text).expect("usage");
        assert!(u.available);
        assert_eq!(u.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(u.context_window, Some(258_400));
        assert_eq!(u.context_tokens, Some(9000));
        assert_eq!(u.total_tokens, Some(16369));
        assert_eq!(u.reasoning_tokens, Some(1552));
        assert_eq!(u.rate_limits.len(), 2);
        assert_eq!(u.rate_limits[0].label, "5-hour");
        assert_eq!(u.rate_limits[1].label, "weekly");
    }

    #[test]
    fn window_labels() {
        assert_eq!(window_label(300), "5-hour");
        assert_eq!(window_label(10080), "weekly");
        assert_eq!(window_label(1440), "1-day");
        assert_eq!(window_label(45), "45-min");
    }

    #[test]
    fn parses_oauth_rate_limits_from_limits_array() {
        let v: Value = serde_json::from_str(
            r#"{
                "five_hour": {"utilization": 68.0, "resets_at": "2026-07-04T03:59:59.737001+00:00"},
                "seven_day": {"utilization": 30.0, "resets_at": "2026-07-07T02:59:59.737029+00:00"},
                "limits": [
                    {"kind": "session", "group": "session", "percent": 68,
                     "resets_at": "2026-07-04T03:59:59.737001+00:00", "scope": null},
                    {"kind": "weekly_all", "group": "weekly", "percent": 30,
                     "resets_at": "2026-07-07T02:59:59.737029+00:00", "scope": null},
                    {"kind": "weekly_scoped", "group": "weekly", "percent": 31,
                     "resets_at": "2026-07-07T02:59:59.737362+00:00",
                     "scope": {"model": {"id": null, "display_name": "Fable"}}},
                    {"kind": "unknown_future_kind", "percent": 5}
                ]
            }"#,
        )
        .unwrap();
        let rl = parse_claude_rate_limits(&v);
        assert_eq!(rl.len(), 3);
        assert_eq!(rl[0].label, "5-hour");
        assert_eq!(rl[0].used_percent, 68.0);
        assert_eq!(rl[0].window_minutes, Some(300));
        assert_eq!(rl[1].label, "weekly");
        assert_eq!(rl[2].label, "weekly (Fable)");
        assert_eq!(rl[2].used_percent, 31.0);
    }

    #[test]
    fn parses_oauth_rate_limits_legacy_shape() {
        let v: Value = serde_json::from_str(
            r#"{
                "five_hour": {"utilization": 12.5, "resets_at": "2026-07-03T04:00:00Z"},
                "seven_day": {"utilization": 61.0, "resets_at": "2026-01-20T00:00:00+00:00"},
                "seven_day_opus": {"utilization": null, "resets_at": null},
                "extra_usage": {"is_enabled": false}
            }"#,
        )
        .unwrap();
        let rl = parse_claude_rate_limits(&v);
        assert_eq!(rl.len(), 2);
        assert_eq!(rl[0].label, "5-hour");
        assert_eq!(rl[0].used_percent, 12.5);
        assert_eq!(rl[0].window_minutes, Some(300));
        assert_eq!(rl[0].resets_at, Some(1_783_051_200));
        assert_eq!(rl[1].label, "weekly");
        assert_eq!(rl[1].resets_at, Some(1_768_867_200));
    }

    #[test]
    fn iso8601_parsing() {
        assert_eq!(
            iso8601_to_unix_secs("2026-07-03T04:00:00Z"),
            Some(1_783_051_200)
        );
        // Fractional seconds and a positive offset.
        assert_eq!(
            iso8601_to_unix_secs("2026-07-03T04:30:15.123+02:00"),
            Some(1_783_045_815)
        );
        // Pre-epoch dates stay correct (div_euclid, not integer division).
        assert_eq!(iso8601_to_unix_secs("1969-12-31T23:59:59Z"), Some(-1));
        assert_eq!(iso8601_to_unix_secs("garbage"), None);
    }

    #[test]
    fn claude_dir_encoding() {
        assert_eq!(
            encode_claude_dir(Path::new("/home/sakuya/dev/agent-workspace")),
            "-home-sakuya-dev-agent-workspace"
        );
    }
}
