//! Fork digests — the context a forked session inherits from its origin.
//!
//! A fork needs to know three things: what the user asked for, what the agent
//! actually changed, and where it stopped. All three are already written down in
//! the agent's own transcript, so we **read** them rather than asking a model to
//! summarize them. The result is exact, instant, free, and works offline.
//!
//! The size is the point. A session's transcript runs to tens of MB, and even the
//! rendered Markdown ([`super::conversation`]) is 40k–300k tokens — too big to
//! paste, too big for most context windows, and expensive to summarize. The
//! digest of that same session is a **few thousand tokens**, because a
//! conversation's *intent* is small: the user's requests are short, the file list
//! is short, and everything in between is the agent working, which the fork does
//! not need to relive.
//!
//! That compactness is what makes everything else possible. The digest is small
//! enough to hand to any agent, small enough to be the entire fallback when no
//! summarizer exists, and small enough that [`crate::summarize`] can turn it into
//! prose in seconds instead of minutes. The LLM summary is a garnish on this; the
//! digest is the meal.

use std::fs;

use serde_json::Value;

use super::conversation::strip_reminders;
use super::title::codex_session_id;
use super::usage::{claude_transcript_path, codex_rollout_path, read_head, TranscriptContext};

/// Longest single user prompt carried verbatim. Prompts are the highest-signal
/// thing in the transcript, so the clip is generous — but a user who pastes a
/// stack trace shouldn't blow up the digest.
const PROMPT_CHARS: usize = 600;
/// Most changed files listed, busiest first.
const MAX_FILES: usize = 40;
/// Most recent shell commands listed.
const MAX_COMMANDS: usize = 15;
/// Clip on the agent's final message.
const TAIL_CHARS: usize = 2000;

/// Claude tools that mean "a file changed".
const CLAUDE_EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// What a fork inherits, before rendering.
#[derive(Debug, Default, PartialEq)]
struct Digest {
    /// Every real user turn, verbatim (clipped), in order.
    prompts: Vec<String>,
    /// Changed files with an edit count, in first-touch order.
    files: Vec<(String, usize)>,
    /// Shell commands, in order.
    commands: Vec<String>,
    /// The agent's last piece of prose — where it left off.
    last_message: String,
}

impl Digest {
    fn prompt(&mut self, text: &str) {
        let t = strip_reminders(text);
        let t = t.trim();
        // A turn that opens with a tag is harness plumbing (Codex wraps
        // `<environment_context>` and friends this way), not something a human
        // typed.
        if t.is_empty() || t.starts_with('<') {
            return;
        }
        self.prompts.push(clip(t, PROMPT_CHARS));
    }

    fn file(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        match self.files.iter_mut().find(|(p, _)| p == path) {
            Some(entry) => entry.1 += 1,
            None => self.files.push((path.to_string(), 1)),
        }
    }

    fn command(&mut self, cmd: &str) {
        if let Some(first) = cmd.lines().find(|l| !l.trim().is_empty()) {
            self.commands.push(clip(first.trim(), 100));
        }
    }

    fn message(&mut self, text: &str) {
        let t = text.trim();
        if !t.is_empty() {
            self.last_message = t.to_string();
        }
    }

    /// `None` when the transcript held nothing a fork could use — no request and
    /// no reply. The caller then falls back to the raw stream (a shell) or to
    /// launching with no context at all, which still beats refusing to fork.
    fn render(mut self) -> Option<String> {
        if self.prompts.is_empty() && self.last_message.is_empty() {
            return None;
        }

        let mut out = String::new();
        if !self.prompts.is_empty() {
            out.push_str("## What this session was asked to do\n\n");
            for p in &self.prompts {
                out.push_str(&format!("- {}\n", indent_continuation(p)));
            }
        }

        if !self.files.is_empty() {
            // Busiest first: the file edited nine times is where the work was.
            // `sort_by_key` is stable, so equal counts keep first-touch order.
            self.files.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            let shown = self.files.len().min(MAX_FILES);
            out.push_str("\n## Files it changed\n\n");
            for (path, n) in &self.files[..shown] {
                let plural = if *n == 1 { "edit" } else { "edits" };
                out.push_str(&format!("- `{path}` ({n} {plural})\n"));
            }
            if self.files.len() > shown {
                out.push_str(&format!(
                    "- … and {} more\n",
                    self.files.len() - shown
                ));
            }
        }

        if !self.commands.is_empty() {
            let start = self.commands.len().saturating_sub(MAX_COMMANDS);
            out.push_str("\n## Recent commands\n\n");
            for c in &self.commands[start..] {
                out.push_str(&format!("- `{c}`\n"));
            }
        }

        if !self.last_message.is_empty() {
            out.push_str("\n## Where it left off\n\n");
            out.push_str(&clip(&self.last_message, TAIL_CHARS));
            out.push('\n');
        }

        Some(out)
    }
}

/// Digest of a Claude Code session, from its own JSONL.
pub fn claude_digest(cx: &TranscriptContext) -> Option<String> {
    let text = fs::read_to_string(claude_transcript_path(cx)?).ok()?;
    parse_claude(&text).render()
}

/// Digest of a Codex session, from its own rollout.
pub fn codex_digest(cx: &TranscriptContext) -> Option<String> {
    let text = fs::read_to_string(codex_rollout_path(cx)?).ok()?;
    parse_codex(&text).render()
}

/// Claude's own conversation id: the `sessionId` carried on every record. We read
/// the field rather than trusting the filename, because the transcript was
/// matched by "newest file in this cwd's directory" — a heuristic good enough for
/// a token counter and not good enough to pick which conversation to resume.
pub fn claude_native_id(cx: &TranscriptContext) -> Option<String> {
    let path = claude_transcript_path(cx)?;
    let head = read_head(&path, 64 * 1024)?;
    for line in head.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(id) = v["sessionId"].as_str().filter(|s| !s.is_empty()) {
            return Some(id.to_string());
        }
    }
    None
}

/// Codex's own conversation id, from the `session_meta` record that opens every
/// rollout.
pub fn codex_native_id(cx: &TranscriptContext) -> Option<String> {
    let path = codex_rollout_path(cx)?;
    codex_session_id(&read_head(&path, 64 * 1024)?)
}

fn parse_claude(text: &str) -> Digest {
    let mut d = Digest::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // Subagent side-threads and harness plumbing are not this conversation.
        if v["isSidechain"] == true || v["isMeta"] == true {
            continue;
        }
        let content = &v["message"]["content"];
        match v["type"].as_str() {
            Some("user") => match content {
                Value::String(s) => d.prompt(s),
                Value::Array(blocks) => {
                    for b in blocks {
                        // A `tool_result` block is the transcript echoing a tool's
                        // output back at the model. It is not the user talking.
                        if b["type"] == "text" {
                            if let Some(t) = b["text"].as_str() {
                                d.prompt(t);
                            }
                        }
                    }
                }
                _ => {}
            },
            Some("assistant") => {
                if let Value::Array(blocks) = content {
                    for b in blocks {
                        match b["type"].as_str() {
                            Some("text") => {
                                if let Some(t) = b["text"].as_str() {
                                    d.message(t);
                                }
                            }
                            Some("tool_use") => {
                                let name = b["name"].as_str().unwrap_or_default();
                                let input = &b["input"];
                                if CLAUDE_EDIT_TOOLS.contains(&name) {
                                    if let Some(p) = input["file_path"].as_str() {
                                        d.file(p);
                                    }
                                } else if name == "Bash" {
                                    if let Some(c) = input["command"].as_str() {
                                        d.command(c);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    d
}

fn parse_codex(text: &str) -> Digest {
    let mut d = Digest::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let p = &v["payload"];
        match (v["type"].as_str(), p["type"].as_str()) {
            (Some("event_msg"), Some("user_message")) => {
                if let Some(m) = p["message"].as_str() {
                    d.prompt(m);
                }
            }
            (Some("event_msg"), Some("agent_message")) => {
                if let Some(m) = p["message"].as_str() {
                    d.message(m);
                }
            }
            // Codex reports applied edits as a `changes` map keyed by absolute
            // path — the same fact Claude spells out one `tool_use` at a time.
            (Some("event_msg"), Some("patch_apply_end")) => {
                if let Value::Object(changes) = &p["changes"] {
                    for path in changes.keys() {
                        d.file(path);
                    }
                }
            }
            (Some("response_item"), Some("function_call")) => {
                // `arguments` is a JSON *string*, not an object.
                let Some(args) = p["arguments"].as_str() else {
                    continue;
                };
                let Ok(a) = serde_json::from_str::<Value>(args) else {
                    continue;
                };
                if let Some(c) = a["cmd"].as_str().or_else(|| a["command"].as_str()) {
                    d.command(c);
                } else if let Value::Array(parts) = &a["command"] {
                    // Some tools pass argv rather than a command line.
                    let joined: Vec<&str> = parts.iter().filter_map(|x| x.as_str()).collect();
                    if !joined.is_empty() {
                        d.command(&joined.join(" "));
                    }
                }
            }
            _ => {}
        }
    }
    d
}

/// Clip to `max` **characters** (not bytes — this text is user prose and may be
/// any script), marking the cut so nobody reads a truncated line as the whole
/// thing.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{} …", head.trim_end())
}

/// Keep a multi-line prompt inside its Markdown bullet: without this, line two of
/// a pasted stack trace escapes the list and reads as a new section.
fn indent_continuation(s: &str) -> String {
    s.replace('\n', "\n  ")
}

/// Compose the file a forked session is pointed at: the LLM's handoff brief when
/// one could be produced, then the digest, then the full prior conversation.
///
/// Ordered cheapest-to-read first. The agent is told to read this file, and the
/// top of it is enough to start; the conversation underneath is there for the
/// questions the digest can't answer ("why did we abandon that approach?").
pub fn brief(
    origin_id: &str,
    agent: &str,
    summary: Option<&str>,
    digest: Option<&str>,
    conversation: Option<&str>,
) -> String {
    let mut out = format!(
        "# Forked session context\n\n\
         This session was forked from an earlier {agent} session (`{origin_id}`) and \
         continues its work. Everything below is that session's context.\n"
    );

    if let Some(s) = summary {
        out.push_str("\n## Handoff brief\n\n");
        out.push_str(s.trim());
        out.push('\n');
    }

    match digest {
        Some(d) => {
            out.push_str("\n---\n\n");
            out.push_str(d.trim());
            out.push('\n');
        }
        None => {
            out.push_str(
                "\n_No structured digest was available for the origin session._\n",
            );
        }
    }

    if let Some(c) = conversation {
        out.push_str("\n---\n\n# Full prior conversation\n\n");
        out.push_str(c.trim());
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_digest_collects_prompts_files_and_the_last_word() {
        let text = concat!(
            r#"{"type":"user","isMeta":true,"message":{"content":"plumbing"}}"#,
            "\n",
            r#"{"type":"user","isSidechain":true,"message":{"content":"a subagent"}}"#,
            "\n",
            r#"{"type":"user","message":{"content":"<system-reminder>x</system-reminder>Add a login page"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"/a/login.tsx"}}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/a/login.tsx"}}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"npm test\nsecond line"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"1 passing"}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Tests pass."}]}}"#,
        );
        let d = parse_claude(text);
        assert_eq!(d.prompts, vec!["Add a login page"], "meta/sidechain/tool_result must not read as user turns");
        assert_eq!(d.files, vec![("/a/login.tsx".to_string(), 2)]);
        assert_eq!(d.commands, vec!["npm test"], "only the command's first line");
        assert_eq!(d.last_message, "Tests pass.");

        let out = d.render().unwrap();
        assert!(out.contains("- Add a login page"));
        assert!(out.contains("`/a/login.tsx` (2 edits)"));
        assert!(out.contains("Tests pass."));
    }

    #[test]
    fn codex_digest_reads_prompts_patches_and_commands() {
        let text = concat!(
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"<environment_context>noise</environment_context>"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Fix the matcher"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"pytest -q\"}"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"patch_apply_end","changes":{"/r/matcher.py":{"type":"update"}}}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"agent_message","message":"Done."}}"#,
        );
        let d = parse_codex(text);
        assert_eq!(d.prompts, vec!["Fix the matcher"], "tagged harness payloads are not user turns");
        assert_eq!(d.files, vec![("/r/matcher.py".to_string(), 1)]);
        assert_eq!(d.commands, vec!["pytest -q"]);
        assert_eq!(d.last_message, "Done.");
    }

    #[test]
    fn busiest_file_leads_and_the_list_is_capped() {
        let mut d = Digest::default();
        d.message("done");
        for i in 0..(MAX_FILES + 5) {
            d.file(&format!("/f/{i}.rs"));
        }
        // One file edited far more than the rest must sort to the top.
        for _ in 0..9 {
            d.file("/f/hot.rs");
        }
        let out = d.render().unwrap();
        let hot = out.find("/f/hot.rs").expect("hot file listed");
        let first = out.find("/f/0.rs").expect("a one-edit file listed");
        assert!(hot < first, "busiest file should lead the list");
        assert!(out.contains("… and 6 more"), "cap must be disclosed, not silent:\n{out}");
    }

    #[test]
    fn a_transcript_with_nothing_to_carry_yields_no_digest() {
        assert_eq!(parse_claude(r#"{"type":"mode","mode":"normal"}"#).render(), None);
        assert_eq!(parse_claude("not json at all").render(), None);
    }

    #[test]
    fn multiline_prompts_stay_inside_their_bullet() {
        let mut d = Digest::default();
        d.prompt("line one\nline two");
        let out = d.render().unwrap();
        assert!(out.contains("- line one\n  line two"), "{out}");
    }

    #[test]
    fn brief_puts_the_summary_above_the_digest_and_the_conversation_last() {
        let b = brief("abc123", "Claude Code", Some("SUMMARY"), Some("DIGEST"), Some("CONVO"));
        let s = b.find("SUMMARY").unwrap();
        let d = b.find("DIGEST").unwrap();
        let c = b.find("CONVO").unwrap();
        assert!(s < d && d < c, "cheapest-to-read must come first:\n{b}");
        assert!(b.contains("abc123"));

        // Degraded shapes still produce a usable document.
        let none = brief("abc123", "Shell", None, None, None);
        assert!(none.contains("No structured digest"));
    }
}
