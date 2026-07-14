//! Turning a fork digest into a prose handoff brief, using an agent CLI the host
//! already has.
//!
//! This is the *optional* half of a fork. The digest ([`crate::plugins::fork`])
//! is exact, instant and always available; the brief here is a nicety on top —
//! it reads the digest and says, in prose, what the session was trying to do and
//! what's left. Every failure path in this module therefore returns `None`, and
//! the fork proceeds with the digest alone. Nothing here is allowed to be the
//! reason a fork doesn't happen.
//!
//! We summarize the **digest**, never the raw conversation. A session's rendered
//! conversation is 40k–300k tokens; its digest is a few thousand. That is the
//! difference between a summarizer that answers in seconds and one that has to
//! chunk, map-reduce, and still silently truncate.
//!
//! ## This runs a real agent, not a chat completion
//!
//! `claude -p`, `codex exec` and `opencode run` are the same agents that edit
//! code — they can call tools. Three consequences, all of them learned the hard
//! way and all of them load-bearing:
//!
//! * **`cwd` is a throwaway directory.** Never the user's worktree. If the
//!   summarizer decides to go poking at files, it finds an empty temp dir.
//! * **stdin is closed.** Given a prompt argument *and* an open stdin, `codex
//!   exec` waits on stdin forever ("Reading additional input from stdin…") and
//!   the fork hangs. `Stdio::null()` is what makes it return.
//! * **there is a deadline.** An agent can think for minutes; a fork cannot.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::plugins::{AgentPlugin, PluginRegistry};

/// How long the summarizer gets before the fork gives up on it and ships the
/// digest alone. Measured: opencode ~6s, codex ~12s, claude ~23s on a digest of a
/// long session. This leaves generous headroom for a slow model or a cold start
/// without ever making the user wait on a wedged CLI.
const DEADLINE: Duration = Duration::from_secs(90);

/// How often we check whether the summarizer has finished.
const POLL: Duration = Duration::from_millis(100);

/// Cap on what we'll read back. A summarizer that ignores "under 200 words" and
/// dumps its context is not going into the brief.
const MAX_OUTPUT: usize = 16 * 1024;

fn prompt_for(digest: &str) -> String {
    // The two constraints in the middle paragraph are not politeness. This agent
    // runs in an empty temp directory (see the module docs), so it can see neither
    // the repository nor the files the digest names. Left to itself it notices
    // that, and writes the fork a paragraph about how its working directory looks
    // nothing like the paths in the digest — advice that is both useless and
    // alarming to the agent that reads the brief. Tell it the digest is all there
    // is, and it writes about the work instead of about its own confusion.
    format!(
        "You are writing a handoff brief so that another AI coding agent can pick up this work \
         in a fresh session.\n\n\
         Below is a digest of the session so far: the user's requests verbatim, the files that \
         changed, and where it left off.\n\n\
         The digest is your ONLY source. You have no working directory, no repository and no \
         access to any of the files it names — do not try to read them, do not use any tools, and \
         do not comment on your own environment, your working directory, or on paths being \
         unavailable. Write only about the work itself.\n\n\
         Write a brief of under 200 words covering: what the goal is, what has already been done, \
         and what still remains. Be concrete and name files where it matters. Output only the \
         brief itself, with no preamble.\n\n\
         ---\n\n{digest}"
    )
}

/// Summarize `digest` into a handoff brief, preferring `preferred` (the agent the
/// fork is being handed to, so the brief is written by the model that will read
/// it) and otherwise any installed agent with a headless mode.
///
/// `None` whenever a brief can't be produced — no agent CLI on this host, the CLI
/// failed, it timed out, or it returned nothing usable. Callers fall back to the
/// digest, which is why every one of those is a `None` and not an error.
///
/// **Blocking.** Call from `spawn_blocking`.
pub fn handoff_brief(
    registry: &PluginRegistry,
    preferred: &str,
    digest: &str,
) -> Option<String> {
    let plugin = summarizer(registry, preferred)?;
    let prompt = prompt_for(digest);

    // A private, empty directory: the agent's cwd, and where `codex exec -o`
    // drops its answer. Removed on every path out, including the timeout.
    let dir = std::env::temp_dir().join(format!("asm-summarize-{}", Uuid::new_v4()));
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let out_path = dir.join("brief.md");
    let spec = plugin.headless(&prompt, &out_path);

    let result =
        spec.and_then(|spec| run(&spec.command, &spec.args, &dir, spec.output_file, DEADLINE));
    let _ = std::fs::remove_dir_all(&dir);

    let brief = clean(&result?);
    (!brief.is_empty()).then_some(brief)
}

/// The agent that will do the summarizing: the fork's target if it has a headless
/// mode, else the first installed agent that does.
fn summarizer(registry: &PluginRegistry, preferred: &str) -> Option<Arc<dyn AgentPlugin>> {
    let usable = |p: &Arc<dyn AgentPlugin>| {
        p.detect_binary().is_some()
            && p.headless("probe", std::path::Path::new("/dev/null")).is_some()
    };
    if let Some(p) = registry.get(preferred).filter(usable) {
        return Some(p);
    }
    registry.agents().iter().find(|p| usable(p)).cloned()
}

/// Run the CLI to completion or to `deadline`, whichever comes first.
fn run(
    command: &str,
    args: &[String],
    cwd: &std::path::Path,
    output_file: Option<PathBuf>,
    deadline: Duration,
) -> Option<String> {
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        // See the module docs: an open stdin wedges `codex exec` indefinitely.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| tracing::warn!("fork summarizer `{command}` failed to start: {e}"))
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    tracing::warn!("fork summarizer `{command}` exited with {status}");
                    return None;
                }
                break;
            }
            Ok(None) => {
                if start.elapsed() >= deadline {
                    tracing::warn!(
                        "fork summarizer `{command}` exceeded {}s; falling back to the digest",
                        deadline.as_secs()
                    );
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(POLL);
            }
            Err(e) => {
                tracing::warn!("fork summarizer `{command}` could not be waited on: {e}");
                return None;
            }
        }
    }

    // `codex exec -o` writes the final message to a file, with none of the
    // progress output that shares its stdout. Everyone else answers on stdout.
    if let Some(path) = output_file {
        return std::fs::read_to_string(path).ok();
    }
    let mut buf = Vec::new();
    child.stdout.take()?.take(MAX_OUTPUT as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Strip what the CLIs wrap around the answer: ANSI colour, and opencode's
/// `> build · <model>` banner.
fn clean(raw: &str) -> String {
    let stripped = strip_ansi(raw);
    let body: Vec<&str> = stripped
        .lines()
        .filter(|l| !l.trim_start().starts_with("> "))
        .collect();
    let mut s = body.join("\n").trim().to_string();
    if s.chars().count() > MAX_OUTPUT {
        s = s.chars().take(MAX_OUTPUT).collect();
    }
    s
}

/// Remove CSI/OSC escape sequences. Small enough to hand-roll, and it keeps the
/// daemon from taking a dependency for one function.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: ESC [ … <final byte in @-~>
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: ESC ] … terminated by BEL or ESC \
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Any other two-byte escape: drop both.
            Some(_) => {}
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_colour_and_osc_but_keeps_prose() {
        assert_eq!(strip_ansi("\x1b[0m\x1b[38;2;1;2;3mhi\x1b[0m"), "hi");
        assert_eq!(strip_ansi("\x1b]0;title\x07body"), "body");
        assert_eq!(strip_ansi("\x1b]0;title\x1b\\body"), "body");
        assert_eq!(strip_ansi("plain"), "plain");
        // A lone ESC at the end must not panic or eat the string.
        assert_eq!(strip_ansi("done\x1b"), "done");
    }

    #[test]
    fn clean_drops_the_opencode_banner() {
        let raw = "\x1b[0m\n> build · glm-5.2\n\x1b[0m\n## Handoff Brief\n\nGoal: ship it.\n";
        assert_eq!(clean(raw), "## Handoff Brief\n\nGoal: ship it.");
    }

    #[test]
    fn clean_of_nothing_is_empty_so_the_caller_falls_back() {
        assert_eq!(clean(""), "");
        assert_eq!(clean("\x1b[0m\n\n"), "");
    }

    #[test]
    fn the_prompt_carries_the_digest_and_fences_the_summarizer_in() {
        let p = prompt_for("## What this session was asked to do\n\n- Fix the bug");
        assert!(p.contains("- Fix the bug"));
        assert!(p.contains("do not use any tools"));
        // Without this, the summarizer notices its empty temp cwd and warns the
        // fork about it — noise in a document the fork is told to trust.
        assert!(p.contains("do not comment on your own environment"));
    }

    #[test]
    fn a_summarizer_that_cannot_start_yields_none_rather_than_erroring() {
        let dir = std::env::temp_dir();
        assert_eq!(run("asm-no-such-binary-xyz", &[], &dir, None, DEADLINE), None);
    }

    #[test]
    fn a_summarizer_that_fails_yields_none() {
        let dir = std::env::temp_dir();
        assert_eq!(run("false", &[], &dir, None, DEADLINE), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_summarizer_that_overruns_its_deadline_is_killed_not_waited_on() {
        // The real DEADLINE is 90s; injecting a short one proves the mechanism
        // without spending 90s to do it. A wedged CLI must never hang the fork.
        let dir = std::env::temp_dir();
        let start = Instant::now();
        let out = run("sleep", &["60".to_string()], &dir, None, Duration::from_millis(300));
        assert_eq!(out, None, "an overrunning summarizer produces no brief");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the fork waited on the child instead of killing it: {:?}",
            start.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn output_is_read_from_the_file_when_the_cli_writes_one() {
        let dir = std::env::temp_dir().join(format!("asm-summarize-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("brief.md");
        let written = run(
            "sh",
            &["-c".into(), format!("printf 'from the file' > {}", out.display())],
            &dir,
            Some(out.clone()),
            DEADLINE,
        );
        assert_eq!(written.as_deref(), Some("from the file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn output_is_read_from_stdout_when_the_cli_writes_none() {
        let dir = std::env::temp_dir();
        let out = run("sh", &["-c".into(), "printf 'on stdout'".into()], &dir, None, DEADLINE);
        assert_eq!(out.as_deref(), Some("on stdout"));
    }
}
