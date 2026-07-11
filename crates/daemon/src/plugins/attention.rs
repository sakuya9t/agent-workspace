//! Attention classification: turning recent terminal output into a
//! working / idle / blocked / error signal.
//!
//! Classification is **per-provider** — it hangs off [`AgentPlugin::attention`]
//! (`super`), so each agent can read its own approval UI. This module is the
//! entry point: it holds the shared default heuristic ([`default_attention`],
//! used by most agents) and re-exports each provider's bespoke matcher from its
//! own submodule ([`claude`], [`codex`]). Add a provider by dropping in a
//! `<name>.rs` submodule and re-exporting its classifier here; the monitor loop
//! (`session_manager`) owns the *byte-stream* mechanics (bell scanning, tail
//! trimming, the idle timer, echo/sticky rules), and the functions here are pure
//! classifiers over a string.
//!
//! [`AgentPlugin::attention`]: super::AgentPlugin::attention

use crate::domain::AttentionState;

mod claude;
mod codex;

pub(crate) use claude::{claude_attention, claude_idle_error};
pub(crate) use codex::codex_attention;

/// The shared default classifier, over the raw decoded output **tail**.
///
/// A session is **blocked** (needs input) when an input prompt sits at the
/// current end of output, or when the agent rang the terminal **bell** — the
/// explicit "I need you" signal agents emit for approval or turn-complete.
/// Otherwise it is working **activity** (later settled to `idle` by the silence
/// timer, or kept "blocked" by the sticky rule in `on_output`).
///
/// Interactive agents render an approval prompt as the question on one line with
/// the answer UI — numbered options, a selection pointer, a surrounding box —
/// on the lines *below* it, so the question is rarely the last non-blank line.
/// We therefore scan the trailing lines upward, matching the patterns on each
/// and skipping past answer-UI [chrome](is_menu_chrome) (options, borders), but
/// stop at the first line of real output. That keeps a prompt-like phrase the
/// agent printed mid-stream — with genuine output after it — reading as
/// activity, not blocked. The bell is applied per chunk (an event), not scanned
/// from the accumulated tail, so a stale bell doesn't linger.
///
/// This tail scan cannot see a prompt whose answer UI includes a line it does
/// not recognise as chrome (e.g. a footer hint below the options) or whose
/// question has scrolled out of the tail behind redraw frames. Providers with
/// such UIs — Claude Code — override [`AgentPlugin::attention`](super::AgentPlugin::attention)
/// with a screen-based matcher ([`claude_attention`]) instead.
pub(crate) fn default_attention(tail: &str, bell: bool) -> (AttentionState, Option<String>) {
    const APPROVAL_PATTERNS: &[&str] = &[
        "(y/n)",
        "[y/n]",
        "(yes/no)",
        "do you want to",
        "proceed?",
        "continue? (",
        "overwrite?",
        "password:",
        "passphrase:",
        "are you sure",
        "press enter to continue",
    ];
    // Upper bound on how far above the last line a prompt's question may sit
    // (question + a handful of options + box borders). Bounded so a stale prompt
    // buried deep in the tail can't be resurrected.
    const MAX_SCAN: usize = 12;
    let mut scanned = 0;
    for line in tail.rsplit(['\n', '\r']) {
        if line.trim().is_empty() {
            continue; // blank padding — not content, and never halts the scan
        }
        if scanned >= MAX_SCAN {
            break;
        }
        scanned += 1;
        let lower = line.to_lowercase();
        for p in APPROVAL_PATTERNS {
            if lower.contains(p) {
                return (
                    AttentionState::ApprovalNeeded,
                    Some(format!("prompt detected: {p}")),
                );
            }
        }
        // Keep climbing past the answer UI to reach the question; a real output
        // line means the trailing text isn't a prompt, so stop here.
        if !is_menu_chrome(line) {
            break;
        }
    }
    if bell {
        return (
            AttentionState::LikelyBlocked,
            Some("agent rang the terminal bell".to_string()),
        );
    }
    (AttentionState::Activity, None)
}

/// True when `line` is part of a prompt's answer UI rather than real output: a
/// numbered option (optionally led by a selection pointer) or a box border /
/// padding. Lets [`default_attention`] scan *past* the options to the question
/// line above, without mistaking ordinary streamed output for a prompt.
fn is_menu_chrome(line: &str) -> bool {
    let is_box = |c: char| ('\u{2500}'..='\u{257f}').contains(&c);
    // Strip a surrounding box and indentation: `│ … │`, `╰──╯`, leading spaces.
    let inner = line
        .trim_matches(|c: char| c.is_whitespace() || is_box(c))
        .trim();
    if inner.is_empty() {
        return true; // pure border or padding line
    }
    // Drop a leading selection pointer / bullet, then require a small integer
    // followed by `.` or `)` — a menu option like "❯ 1. Yes" or "2) No".
    let opt = inner
        .trim_start_matches(|c: char| {
            matches!(
                c,
                '\u{276f}' | '>' | '\u{25b6}' | '\u{2192}' | '*' | '-' | '\u{2022}'
            )
        })
        .trim_start();
    let digits = opt.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && matches!(opt[digits..].chars().next(), Some('.') | Some(')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- default (tail) heuristic ----

    #[test]
    fn detects_approval_prompt_at_end() {
        let (a, reason) = default_attention("Proceed? (y/n)", false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
        assert!(reason.is_some());
        // A prompt on a prior line (cursor sits after it, no trailing output).
        let (a2, _) = default_attention("Working...\nPassword: ", false);
        assert_eq!(a2, AttentionState::ApprovalNeeded);
    }

    #[test]
    fn prompt_phrase_mid_stream_is_activity() {
        // The prompt-like phrase is NOT the last line — the agent kept working,
        // so it must read as active, not blocked (no active<->blocked flicker).
        let (a, _) = default_attention("Do you want to continue?\nDownloading 42%...", false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn repro_multiline_menu_prompt_is_blocked() {
        // An agent renders an approval prompt as a question line followed by
        // numbered options, so the *last* non-blank line is an option ("2. No"),
        // not the question. Matching only the last line missed it.
        let prompt = "Do you want to proceed?\n\u{276f} 1. Yes\n  2. No, and tell Claude what to do differently";
        let (a, _) = default_attention(prompt, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
    }

    #[test]
    fn repro_boxed_menu_prompt_is_blocked() {
        // Same shape, wrapped in a rounded box: question and options each sit
        // inside `│ … │`, and the last line is the box's bottom border.
        let prompt = "\u{256d}────────────────╮\n\
                      │ Do you want to proceed?        │\n\
                      │ \u{276f} 1. Yes                       │\n\
                      │   2. No                        │\n\
                      \u{2570}────────────────╯";
        let (a, _) = default_attention(prompt, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
    }

    #[test]
    fn trailing_numbered_list_output_is_activity() {
        // Skipping *past* numbered options to find the question must not turn an
        // ordinary numbered list the agent printed into a blocked prompt: the
        // line above the list carries no approval phrase, so the scan stops there.
        let out = "Here is the plan:\n1. Read the file\n2. Edit it\n3. Run tests";
        let (a, _) = default_attention(out, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn plain_output_is_activity() {
        let (a, reason) = default_attention("building project...", false);
        assert_eq!(a, AttentionState::Activity);
        assert!(reason.is_none());
    }

    #[test]
    fn bell_signals_blocked() {
        // The bell is the agent explicitly asking for attention.
        let (a, _) = default_attention("some output", true);
        assert_eq!(a, AttentionState::LikelyBlocked);
    }
}
