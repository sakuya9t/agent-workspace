//! Attention classification: turning recent terminal output into a
//! working / idle / blocked / error signal.
//!
//! Classification is **per-provider** — it hangs off [`AgentPlugin::attention`]
//! (`super`), so each agent can read its own approval UI. This module holds the
//! shared default heuristic used by most agents plus Claude Code's bespoke
//! matcher. The monitor loop (`session_manager`) owns the *byte-stream*
//! mechanics (bell scanning, tail trimming, the idle timer, echo/sticky rules);
//! the functions here are pure classifiers over a string.

use crate::domain::AttentionState;

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

/// Claude Code's screen-based approval matcher, over the rendered visible
/// **screen** (rows joined by `\n`).
///
/// Claude renders an approval prompt as a question line with a numbered
/// `❯`-selected option menu below it and a keybinding-hint footer under *that*
/// (`Esc to cancel · Tab to amend · …`). Both the options and that footer sit
/// below the question, and the daemon's raw output tail is a stream of redraw
/// frames — so [`default_attention`]'s upward scan halts on the footer (which it
/// doesn't recognise as menu chrome) or on a spinner redraw frame before ever
/// reaching the question, and worse, the question scrolls out of the 4 KB tail
/// behind the ~0.6 s spinner frames within ~40 s. So the prompt reads as
/// "working". Classifying from the rendered screen fixes both: it is bounded to
/// the visible grid and always reflects the latest frame.
///
/// The unambiguous "waiting for a choice" signal is the selection pointer on a
/// numbered option (`❯ 1. Yes`); prose that merely contains "do you want to"
/// never renders one. We require *both* the pointer and an approval phrase so an
/// ordinary numbered list Claude printed isn't mistaken for a prompt. The whole
/// bounded screen is scanned (not just the trailing line), so where the question
/// sits relative to the options and footer no longer matters. Falls back to the
/// bell, the same as [`default_attention`].
pub(crate) fn claude_attention(screen: &str, bell: bool) -> (AttentionState, Option<String>) {
    const APPROVAL_PHRASES: &[&str] = &[
        // "Do you want to proceed?" / "…make this edit to X?" / "…create X?".
        "do you want to",
        "this command requires approval",
    ];
    if screen.lines().any(is_selected_option) {
        for line in screen.lines() {
            let lower = line.to_lowercase();
            if let Some(p) = APPROVAL_PHRASES.iter().find(|p| lower.contains(**p)) {
                return (
                    AttentionState::ApprovalNeeded,
                    Some(format!("prompt detected: {p}")),
                );
            }
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

/// Claude Code's stalled-on-error matcher, over the rendered visible **screen**.
///
/// When the API fails mid-turn Claude Code prints `● API Error: …` and simply
/// stops — no bell, no prompt — so the silence timer used to settle the session
/// to a calm "idle" even though the turn died. This runs at that settle moment
/// ([`AgentPlugin::idle_error`](super::AgentPlugin::idle_error), called from the
/// monitor's idle tick) and answers "did it stop *on an error*?".
///
/// The error must be the screen's **trailing content**, not merely visible:
/// captured real screens show an old `API Error` line sitting mid-screen long
/// after the user retried and a later turn streamed below it, so a whole-screen
/// match would keep flagging a recovered session. We anchor at the input area —
/// everything below the last box-drawing line (its bottom border) is footer
/// hints — and climb upward past chrome: borders, the `❯` input line, and the
/// spinner/status line (`✻ Worked for 32m 22s`, which stays on the frozen
/// frame). The first `●`-bulleted line then decides: an `API Error` matches,
/// any other response line means the turn ended normally. Un-bulleted text
/// keeps the climb alive so an error message wrapped across lines is still
/// found via its bulleted first line; a `⎿` continuation is checked too (the
/// mid-retry `⎿ API Error … · Retrying…` shape) but otherwise skipped as tool
/// output / tips.
pub(crate) fn claude_idle_error(screen: &str) -> Option<String> {
    // The error sits within a couple of content lines of the input area; deep
    // scans would only resurrect stale text.
    const MAX_SCAN: usize = 15;
    // Spinner/status-line glyphs, both animating ("✽ Spinning…") and at rest
    // ("✻ Worked for 32m 22s").
    const STATUS_GLYPHS: &[char] = &['\u{b7}', '✢', '✳', '✶', '✻', '✽', '✺', '∗', '*'];
    const BULLETS: &[char] = &['\u{25cf}', '\u{23fa}']; // ● / ⏺ turn bullets
    let is_box = |c: char| ('\u{2500}'..='\u{257f}').contains(&c);

    let lines: Vec<&str> = screen.lines().collect();
    // The input area's bottom border is the last box-drawing line on screen
    // (footer hints render below it). No input area, nothing at rest to read.
    let anchor = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with(is_box))?;

    let mut scanned = 0;
    for line in lines[..anchor].iter().rev() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if scanned >= MAX_SCAN {
            break;
        }
        scanned += 1;
        if t.starts_with(is_box) || t.starts_with('\u{276f}') || t.starts_with(STATUS_GLYPHS) {
            continue; // input-area chrome or the spinner/status line
        }
        if let Some(rest) = t.strip_prefix(BULLETS).map(str::trim_start) {
            // The trailing content block's bulleted header decides.
            return rest.starts_with("API Error").then(|| rest.to_string());
        }
        if let Some(rest) = t.strip_prefix('\u{23bf}').map(str::trim_start) {
            if rest.starts_with("API Error") {
                return Some(rest.to_string());
            }
            continue; // ⎿ tool output / tip attached to the line above
        }
        // Plain text: possibly the wrapped tail of the bulleted line above —
        // keep climbing.
    }
    None
}

/// A selection-pointer numbered menu option like "❯ 1. Yes" or "> 2. No": a
/// `❯`/`>` pointer (after any indent), then a small integer followed by `.`
/// or `)`. This is the signal that an interactive menu is *awaiting a choice*.
fn is_selected_option(line: &str) -> bool {
    let rest = line.trim_start();
    let rest = match rest
        .strip_prefix('\u{276f}')
        .or_else(|| rest.strip_prefix('>'))
    {
        Some(r) => r.trim_start(),
        None => return false,
    };
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && matches!(rest[digits..].chars().next(), Some('.') | Some(')'))
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

    // ---- Claude screen matcher ----

    /// The captured real-world screen from the reported bug: a full-width `─`
    /// top border (no side/bottom box), the question, a `❯`-selected option
    /// menu, and a keybinding-hint footer *below* the options. The default tail
    /// scan halts on that footer; the screen matcher must still see the prompt.
    #[test]
    fn claude_permission_screen_with_footer_is_approval() {
        let screen = "\
 This command requires approval\n\
\n\
 Do you want to proceed?\n\
 \u{276f} 1. Yes\n\
   2. Yes, and don't ask again for similar commands in /home/sakuya/x\n\
   3. No\n\
\n\
 Esc to cancel \u{b7} Tab to amend \u{b7} ctrl+e to explain";
        let (a, reason) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
        // Either approval line on the screen is a valid reason; the top-down scan
        // reports "This command requires approval" (it sits above the question).
        assert!(reason.unwrap().starts_with("prompt detected: "));
    }

    #[test]
    fn claude_edit_prompt_is_approval() {
        let screen = " Do you want to make this edit to session_manager.rs?\n \u{276f} 1. Yes\n   2. No";
        let (a, _) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
    }

    #[test]
    fn claude_prose_without_menu_is_activity() {
        // Claude narrating "do you want to …" in prose renders no selection
        // pointer, so it must not read as a blocked prompt.
        let screen = "I can refactor this. Do you want to keep the old API too?\nLet me know.";
        let (a, _) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn claude_numbered_list_without_pointer_is_activity() {
        // A plain numbered list (no `❯` selection pointer) is output, not a menu.
        let screen = "Plan:\n 1. Read\n 2. Edit\n 3. Test";
        let (a, _) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn claude_menu_without_phrase_is_activity() {
        // A selection menu with no approval phrase is left to the bell fallback,
        // not force-classified as an approval (e.g. an unrelated picker).
        let screen = " Pick a theme:\n \u{276f} 1. Dark\n   2. Light";
        let (a, _) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn claude_bell_is_blocked() {
        let (a, _) = claude_attention("just working", true);
        assert_eq!(a, AttentionState::LikelyBlocked);
    }

    // ---- Claude stalled-on-error (idle settle) ----

    /// The captured real-world frame from the reported bug: the API died
    /// mid-turn, Claude printed the error and froze — with the stale status
    /// line still below it — and the session then read as a calm "idle".
    #[test]
    fn claude_stalled_api_error_screen_is_error() {
        let screen = "\
● The old marker scrolled out of view. Let me make the probe type its own fresh marker:\n\
\n\
● API Error: Server error mid-response. The response above may be incomplete.\n\
\n\
✻ Worked for 32m 22s\n\
\n\
────────────────────────────────────────\n\
❯ \n\
────────────────────────────────────────\n\
  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents";
        let reason = claude_idle_error(screen).unwrap();
        assert!(reason.starts_with("API Error: Server error mid-response"));
    }

    #[test]
    fn claude_recovered_screen_with_stale_error_above_is_not_error() {
        // Also captured live: after the user retried, the old error line stays
        // visible mid-screen while the next turn's output streams below it.
        // Only the *trailing* content block may flag — here it's the new turn.
        let screen = "\
● API Error: Server error mid-response. The response above may be incomplete.\n\
\n\
● Making 2 scratchpad edits +20 -6, running 1 shell command…\n\
  ⎿  $ cd /tmp/scratch && node rightclick-debug.mjs\n\
\n\
✽ Spinning… (1m 26s · ↓ 1.5k tokens)\n\
\n\
────────────────────────────────────────\n\
❯ \n\
────────────────────────────────────────\n\
  ⏵⏵ bypass permissions on · 1 shell · esc to interrupt";
        assert_eq!(claude_idle_error(screen), None);
    }

    #[test]
    fn claude_wrapped_error_line_is_error() {
        // On a narrow terminal the error message wraps; the un-bulleted
        // continuation must not stop the climb before the bulleted header.
        let screen = "\
● API Error: Server error mid-response. The response\n\
  above may be incomplete.\n\
\n\
✻ Worked for 5s\n\
────────────────────\n\
❯ \n\
────────────────────";
        assert!(claude_idle_error(screen).is_some());
    }

    #[test]
    fn claude_prose_quoting_api_error_is_not_error() {
        // A successful turn whose *text* mentions "API Error" (e.g. this very
        // feature being discussed) must not flag: the bulleted header decides,
        // and it doesn't start with the error marker.
        let screen = "\
● I added handling for the \"API Error: Server error mid-response.\" message.\n\
\n\
✻ Worked for 3s\n\
────────────────────\n\
❯ \n\
────────────────────\n\
  ? for shortcuts";
        assert_eq!(claude_idle_error(screen), None);
    }

    #[test]
    fn claude_retry_error_continuation_is_error() {
        // The mid-retry shape attaches the error as a ⎿ continuation. If the
        // console freezes there, it still counts.
        let screen = "\
● Let me try that again.\n\
  ⎿  API Error (Request timed out.) · Retrying in 4 seconds… (attempt 3/10)\n\
────────────────────\n\
❯ \n\
────────────────────";
        assert!(claude_idle_error(screen).is_some());
    }

    #[test]
    fn claude_screen_without_input_area_is_not_error() {
        assert_eq!(claude_idle_error("plain scrollback, no box"), None);
        assert_eq!(claude_idle_error(""), None);
    }

    #[test]
    fn selected_option_shapes() {
        assert!(is_selected_option(" \u{276f} 1. Yes"));
        assert!(is_selected_option("> 2) No"));
        assert!(!is_selected_option("  2. Yes, and don't ask again")); // not selected
        assert!(!is_selected_option(" Do you want to proceed?"));
    }
}
