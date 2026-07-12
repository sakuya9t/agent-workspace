//! Readable conversation export — what "Save conversation" hands the user.
//!
//! The recorded PTY stream is *not* a transcript. It is what a TUI sent a
//! terminal, and both agents drive one (see `docs/terminal-scrollback.md`):
//! Claude paints from the **alternate screen**, which by definition has no
//! scrollback, so its stream is a repaint movie — tens of MB of frames of the
//! same screen, of which only the last frame is coherent. Codex scrolls the
//! normal buffer inside a bottom-margin region, so its history is in there, but
//! interleaved with composer repaints. Handing either to a text editor gives the
//! user a wall of `ESC[38;2;…m` and `ESC[60G`, and stripping the escapes just
//! turns that into noise: the frames stay, the prose doesn't appear.
//!
//! So the export reads the agent's own file instead. Both agents persist a
//! structured per-session JSONL, and [`super::usage`] already locates it for the
//! token counters; this module renders that same file to Markdown — user turns,
//! agent turns, and the tool calls between them.
//!
//! Tool inputs and outputs are clipped ([`CLIP_LINES`] / [`CLIP_CHARS`]). The
//! export is meant to be read — and pasted into a fresh session to carry context
//! over — and a verbatim tool dump is what makes the agent's own file unreadable
//! in the first place.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::usage::{claude_transcript_path, codex_rollout_path, TranscriptContext};

/// Per-block clip for tool inputs and outputs: enough to see what ran and how it
/// came back, without pasting a whole file listing into the document.
const CLIP_LINES: usize = 12;
const CLIP_CHARS: usize = 800;

/// Longest document title we synthesize from a first user message.
const TITLE_CHARS: usize = 72;

/// Render a Claude Code session's own transcript as Markdown. `None` when no
/// transcript can be matched to this session (the agent never wrote one, or it
/// holds nothing but scaffolding) — the caller then falls back to the raw stream.
pub fn claude_conversation(cx: &TranscriptContext) -> Option<String> {
    let path = claude_transcript_path(cx)?;
    let text = fs::read_to_string(&path).ok()?;
    let body = render_claude(&text, "Claude");
    if body.trim().is_empty() {
        return None;
    }
    let title = claude_title(&text).unwrap_or_else(|| "Claude Code session".into());
    Some(document(&title, "Claude Code", &cx.cwd, &path, &body))
}

/// Render a Codex session's own rollout as Markdown. `None` as above.
pub fn codex_conversation(cx: &TranscriptContext) -> Option<String> {
    let path = codex_rollout_path(cx)?;
    let text = fs::read_to_string(&path).ok()?;
    let body = render_codex(&text, "Codex");
    if body.trim().is_empty() {
        return None;
    }
    let title = first_user_line(&body).unwrap_or_else(|| "Codex session".into());
    Some(document(&title, "Codex", &cx.cwd, &path, &body))
}

/// Wrap a rendered body in a header naming the session, so a file read months
/// later still says where it came from and what was left out.
fn document(title: &str, agent: &str, cwd: &Path, source: &Path, body: &str) -> String {
    format!(
        "# {title}\n\n\
         {agent} conversation in `{}`.\n\n\
         Rendered by asm from the agent's own transcript (`{}`). Tool inputs and outputs are \
         clipped to {CLIP_LINES} lines; agent reasoning and subagent side-threads are omitted.\n\n\
         ---\n\n{body}",
        cwd.display(),
        source.display(),
    )
}

// ---------- Claude Code ----------

/// The title Claude gave the session (it rewrites `ai-title` as the topic
/// firms up, so the last one wins).
fn claude_title(text: &str) -> Option<String> {
    let mut title = None;
    for line in text.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v["type"] == "ai-title" {
            if let Some(t) = v["aiTitle"].as_str().filter(|t| !t.is_empty()) {
                title = Some(t.to_string());
            }
        }
    }
    title
}

/// Claude's JSONL: one record per content block, plus bookkeeping records
/// (`mode`, `ai-title`, `file-history-snapshot`, …) that carry no conversation.
fn render_claude(text: &str, agent: &str) -> String {
    let mut md = Md::new(agent);
    for line in text.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Sidechains are subagent threads and `isMeta` records are UI plumbing;
        // neither is part of the conversation the user had.
        if v["isSidechain"] == true || v["isMeta"] == true {
            continue;
        }
        let ts = v["timestamp"].as_str();
        let role = v["type"].as_str().unwrap_or_default();
        let content = &v["message"]["content"];

        match (role, content) {
            ("user", Value::String(s)) => md.turn(Speaker::User, ts, &strip_reminders(s)),
            ("user", Value::Array(blocks)) => {
                for b in blocks {
                    match b["type"].as_str() {
                        // A tool result is the tool answering the agent, not a
                        // user turn — it stays inside the agent's flow.
                        Some("tool_result") => md.tool_result(&block_text(&b["content"])),
                        Some("text") => md.turn(
                            Speaker::User,
                            ts,
                            &strip_reminders(b["text"].as_str().unwrap_or_default()),
                        ),
                        Some("image") => md.tool_result("[image]"),
                        _ => {}
                    }
                }
            }
            ("assistant", Value::Array(blocks)) => {
                for b in blocks {
                    match b["type"].as_str() {
                        Some("text") => {
                            md.turn(Speaker::Agent, ts, b["text"].as_str().unwrap_or_default())
                        }
                        Some("tool_use") => {
                            md.speak(Speaker::Agent, ts);
                            md.tool_call(b["name"].as_str().unwrap_or("tool"), &b["input"]);
                        }
                        // `thinking` is signed, often empty, and not conversation.
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    md.out
}

/// Claude appends `<system-reminder>` blocks to user turns; they're harness
/// plumbing the user never typed, and they dwarf the prompt in the export.
fn strip_reminders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<system-reminder>") {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find("</system-reminder>") {
            Some(end) => &rest[start + end + "</system-reminder>".len()..],
            None => "", // unterminated — drop the tail
        };
    }
    out.push_str(rest);
    out.trim().to_string()
}

// ---------- Codex ----------

/// Codex's rollout: `event_msg` records carry the conversation as plain strings,
/// `response_item` records carry the model-facing wire format. The messages are
/// duplicated across both, so take prose from `event_msg` and tool calls from
/// `response_item` — anything else (reasoning, token counts, world state) is not
/// conversation.
fn render_codex(text: &str, agent: &str) -> String {
    let mut md = Md::new(agent);
    for line in text.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts = v["timestamp"].as_str();
        let p = &v["payload"];
        let kind = p["type"].as_str().unwrap_or_default();

        match (v["type"].as_str().unwrap_or_default(), kind) {
            ("event_msg", "user_message") => {
                md.turn(Speaker::User, ts, p["message"].as_str().unwrap_or_default())
            }
            ("event_msg", "agent_message") => md.turn(
                Speaker::Agent,
                ts,
                p["message"].as_str().unwrap_or_default(),
            ),
            ("response_item", "custom_tool_call") => {
                md.speak(Speaker::Agent, ts);
                md.tool_call(p["name"].as_str().unwrap_or("tool"), &p["input"]);
            }
            ("response_item", "function_call") => {
                md.speak(Speaker::Agent, ts);
                md.tool_call(p["name"].as_str().unwrap_or("tool"), &p["arguments"]);
            }
            ("response_item", "custom_tool_call_output" | "function_call_output") => {
                md.tool_result(&block_text(&p["output"]))
            }
            _ => {}
        }
    }
    md.out
}

/// First user line of a rendered body, for a Codex document title (Codex records
/// no title of its own). Clipped on a word boundary — a title cut mid-word reads
/// like a bug.
fn first_user_line(body: &str) -> Option<String> {
    let mut lines = body.lines().skip_while(|l| !l.starts_with("## User"));
    lines.next()?; // the heading
    let first = lines.map(str::trim).find(|l| !l.is_empty())?;
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

// ---------- Markdown assembly ----------

#[derive(Clone, Copy, PartialEq)]
enum Speaker {
    User,
    Agent,
}

/// Accumulates the document. Headings are emitted per *speaker change*, not per
/// record: both agents write one record per content block, so a turn that says
/// something, calls three tools, then says more is one heading, not five.
struct Md {
    out: String,
    current: Option<Speaker>,
    /// What to call the agent in its headings ("Claude", "Codex").
    agent: String,
}

impl Md {
    fn new(agent: &str) -> Self {
        Self {
            out: String::new(),
            current: None,
            agent: agent.to_string(),
        }
    }

    /// Open a turn for `who` if they don't already hold the floor.
    fn speak(&mut self, who: Speaker, ts: Option<&str>) {
        if self.current == Some(who) {
            return;
        }
        self.current = Some(who);
        let name = match who {
            Speaker::User => "User",
            Speaker::Agent => &self.agent,
        };
        match ts {
            Some(t) => self
                .out
                .push_str(&format!("## {name} · {}\n\n", short_ts(t))),
            None => self.out.push_str(&format!("## {name}\n\n")),
        }
    }

    /// A prose turn. Empty text is dropped — an empty heading helps nobody.
    fn turn(&mut self, who: Speaker, ts: Option<&str>, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.speak(who, ts);
        self.out.push_str(text.trim());
        self.out.push_str("\n\n");
    }

    /// A tool call: the name, the agent's one-line description of it when there
    /// is one, then whatever identifies the call (a command, a path, or the raw
    /// arguments), clipped.
    fn tool_call(&mut self, name: &str, input: &Value) {
        let (lang, body) = tool_input(input);
        match input["description"].as_str().filter(|d| !d.is_empty()) {
            Some(d) => self.out.push_str(&format!("**{name}** — {d}\n\n")),
            None => self.out.push_str(&format!("**{name}**\n\n")),
        }
        if !body.trim().is_empty() {
            self.out.push_str(&fenced(lang, &clip(&body)));
        }
    }

    /// What the tool returned, clipped. Never opens a turn: the result belongs
    /// to whoever called it.
    fn tool_result(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.out.push_str(&fenced("", &clip(text)));
    }
}

/// Render a tool's input as `(fence language, text)`. Handles both agents'
/// shapes: Codex passes a bare string, Claude an object whose interesting field
/// is usually a command or a path.
fn tool_input(input: &Value) -> (&'static str, String) {
    match input {
        Value::String(s) => ("", s.clone()),
        Value::Object(_) => {
            if let Some(cmd) = input["command"].as_str() {
                ("sh", cmd.to_string())
            } else if let Some(path) = input["file_path"].as_str() {
                ("", path.to_string())
            } else {
                (
                    "json",
                    serde_json::to_string_pretty(input).unwrap_or_default(),
                )
            }
        }
        Value::Null => ("", String::new()),
        other => ("json", other.to_string()),
    }
}

/// Flatten a content field that may be a string, a list of `{text}` blocks
/// (both agents use this for tool output), or anything else.
fn block_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str().or_else(|| b.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Clip to [`CLIP_LINES`] / [`CLIP_CHARS`], saying what was dropped rather than
/// truncating silently.
fn clip(text: &str) -> String {
    let text = text.trim_end();
    let total = text.lines().count();
    let mut out: String = text.lines().take(CLIP_LINES).collect::<Vec<_>>().join("\n");
    let mut dropped_lines = total.saturating_sub(CLIP_LINES);

    if out.chars().count() > CLIP_CHARS {
        out = out.chars().take(CLIP_CHARS).collect();
        // The char cut can land mid-line; count that line as dropped too.
        dropped_lines = total.saturating_sub(out.lines().count());
    }
    if dropped_lines > 0 {
        out.push_str(&format!("\n… [{dropped_lines} more lines]"));
    }
    out
}

/// A fenced block whose fence is always longer than any backtick run inside, so
/// code containing fences (every conversation about Markdown) can't break out.
fn fenced(lang: &str, body: &str) -> String {
    let longest = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}{lang}\n{}\n{fence}\n\n", body.trim_end())
}

/// `2026-07-12T06:29:10.593Z` → `2026-07-12 06:29:10`.
fn short_ts(ts: &str) -> String {
    let t = ts.split('.').next().unwrap_or(ts).trim_end_matches('Z');
    t.replacen('T', " ", 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &str = r#"
{"type":"ai-title","aiTitle":"Fix the transcript export"}
{"type":"user","timestamp":"2026-07-12T06:29:10.593Z","message":{"role":"user","content":"Make it readable.<system-reminder>ignore me</system-reminder>"}}
{"type":"assistant","timestamp":"2026-07-12T06:29:12.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"secret","signature":"abc"}]}}
{"type":"assistant","timestamp":"2026-07-12T06:29:13.000Z","message":{"role":"assistant","content":[{"type":"text","text":"On it."}]}}
{"type":"assistant","timestamp":"2026-07-12T06:29:14.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls docs/","description":"List docs"}}]}}
{"type":"user","timestamp":"2026-07-12T06:29:15.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"backlog.md\nsetup.md"}]}}
{"type":"assistant","timestamp":"2026-07-12T06:29:16.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Two files."}]}}
{"type":"user","isSidechain":true,"message":{"role":"user","content":"subagent chatter"}}
{"type":"mode","mode":"normal"}
"#;

    #[test]
    fn claude_renders_turns_tools_and_results() {
        let md = render_claude(CLAUDE, "Claude");
        assert!(md.contains("## User · 2026-07-12 06:29:10"), "{md}");
        assert!(md.contains("Make it readable."));
        assert!(md.contains("**Bash**"));
        assert!(md.contains("ls docs/"));
        assert!(md.contains("backlog.md"));
        assert!(md.contains("Two files."));
    }

    #[test]
    fn claude_omits_noise() {
        let md = render_claude(CLAUDE, "Claude");
        assert!(!md.contains("secret"), "thinking leaked: {md}");
        assert!(!md.contains("ignore me"), "system-reminder leaked: {md}");
        assert!(!md.contains("subagent chatter"), "sidechain leaked: {md}");
    }

    #[test]
    fn one_heading_per_turn_not_per_block() {
        // "On it." → Bash → result → "Two files." is ONE agent turn.
        let md = render_claude(CLAUDE, "Claude");
        assert_eq!(md.matches("## Claude").count(), 1, "{md}");
        assert_eq!(md.matches("## User").count(), 1, "{md}");
    }

    #[test]
    fn claude_title_takes_the_last_one() {
        let text = format!(
            "{CLAUDE}\n{}",
            r#"{"type":"ai-title","aiTitle":"Final title"}"#
        );
        assert_eq!(claude_title(&text).as_deref(), Some("Final title"));
    }

    #[test]
    fn tool_calls_carry_the_agents_description() {
        let md = render_claude(CLAUDE, "Claude");
        assert!(md.contains("**Bash** — List docs"), "{md}");
    }

    #[test]
    fn synthesized_title_breaks_on_a_word() {
        let body = format!("## User · now\n\n{}\n", "alpha beta gamma ".repeat(10));
        let title = first_user_line(&body).unwrap();
        assert!(
            title.chars().count() <= TITLE_CHARS + 1,
            "too long: {title}"
        );
        assert!(title.ends_with('…'), "{title}");
        // Whole words only — never a "gam…".
        let words = title.trim_end_matches('…');
        assert!(
            words
                .split(' ')
                .all(|w| ["alpha", "beta", "gamma", ""].contains(&w)),
            "cut mid-word: {title}"
        );
    }

    const CODEX: &str = r#"
{"timestamp":"2026-07-12T06:32:05.569Z","type":"event_msg","payload":{"type":"user_message","message":"Design a button icon."}}
{"timestamp":"2026-07-12T06:32:09.544Z","type":"response_item","payload":{"type":"reasoning","encrypted_content":"secret"}}
{"timestamp":"2026-07-12T06:32:10.163Z","type":"event_msg","payload":{"type":"agent_message","message":"I'll inspect the UI."}}
{"timestamp":"2026-07-12T06:32:10.163Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I'll inspect the UI."}]}}
{"timestamp":"2026-07-12T06:32:11.834Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"pwd && ls"}}
{"timestamp":"2026-07-12T06:32:11.946Z","type":"response_item","payload":{"type":"custom_tool_call_output","output":[{"type":"input_text","text":"/repo\nclient"}]}}
{"timestamp":"2026-07-12T06:32:11.946Z","type":"event_msg","payload":{"type":"token_count","info":{}}}
"#;

    #[test]
    fn codex_renders_prose_and_tools_without_duplicates() {
        let md = render_codex(CODEX, "Codex");
        assert!(md.contains("## User · 2026-07-12 06:32:05"), "{md}");
        assert!(md.contains("Design a button icon."));
        assert!(md.contains("**exec**"));
        assert!(md.contains("pwd && ls"));
        assert!(md.contains("/repo"));
        assert!(!md.contains("secret"), "reasoning leaked: {md}");
        // The message appears as both event_msg and response_item; render once.
        assert_eq!(md.matches("I'll inspect the UI.").count(), 1, "{md}");
    }

    #[test]
    fn long_output_is_clipped_and_says_so() {
        let long = (1..=100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = clip(&long);
        assert!(out.contains("line 12"));
        assert!(!out.contains("line 13"));
        assert!(out.contains("[88 more lines]"), "{out}");
    }

    #[test]
    fn char_clip_reports_the_lines_it_cut() {
        let long = (1..=20)
            .map(|_| "x".repeat(200))
            .collect::<Vec<_>>()
            .join("\n");
        let out = clip(&long);
        assert!(
            out.chars().count() <= CLIP_CHARS + 32,
            "clip overshot: {}",
            out.len()
        );
        assert!(out.contains("more lines]"), "{out}");
    }

    #[test]
    fn fence_outgrows_backticks_in_the_body() {
        let body = "before\n```sh\necho hi\n```\nafter";
        let out = fenced("", body);
        assert!(out.starts_with("````\n"), "{out}");
        assert!(out.trim_end().ends_with("\n````"), "{out}");
    }

    #[test]
    fn empty_transcript_renders_nothing() {
        assert!(
            render_claude("{\"type\":\"mode\",\"mode\":\"normal\"}", "Claude")
                .trim()
                .is_empty()
        );
        assert!(render_codex("not json\n", "Codex").trim().is_empty());
    }
}
