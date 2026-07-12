//! Claude Code's attention classifiers.
//!
//! Claude opts into screen-based classification ([`super`] module doc): its
//! approval UI (a boxed selection menu under a footer) and its stall-on-error
//! rendering can't be read from the raw output tail, so both matchers here work
//! over the rendered visible **screen** instead.

use crate::domain::AttentionState;

/// Claude Code's screen-based approval matcher, over the rendered visible
/// **screen** (rows joined by `\n`).
///
/// Claude renders an approval prompt as a question line with a numbered
/// `❯`-selected option menu below it and a keybinding-hint footer under *that*
/// (`Esc to cancel · Tab to amend · …`). Both the options and that footer sit
/// below the question, and the daemon's raw output tail is a stream of redraw
/// frames — so [`super::default_attention`]'s upward scan halts on the footer
/// (which it doesn't recognise as menu chrome) or on a spinner redraw frame
/// before ever reaching the question, and worse, the question scrolls out of the
/// 4 KB tail behind the ~0.6 s spinner frames within ~40 s. So the prompt reads
/// as "working". Classifying from the rendered screen fixes both: it is bounded
/// to the visible grid and always reflects the latest frame.
///
/// The unambiguous "waiting for a choice" signal is the selection pointer on a
/// numbered option (`❯ 1. Yes`); prose that merely contains "do you want to"
/// never renders one. We require *both* the pointer and an approval phrase so an
/// ordinary numbered list Claude printed isn't mistaken for a prompt. The whole
/// bounded screen is scanned (not just the trailing line), so where the question
/// sits relative to the options and footer no longer matters. Falls back to the
/// bell, the same as [`super::default_attention`].
pub(crate) fn claude_attention(screen: &str, bell: bool) -> (AttentionState, Option<String>) {
    const APPROVAL_PHRASES: &[&str] = &[
        // "Do you want to proceed?" / "…make this edit to X?" / "…create X?".
        "do you want to",
        // Plan mode's exit prompt phrases the same question differently: "Claude
        // has written up a plan and is ready to execute. Would you like to
        // proceed?".
        "would you like to",
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
        // A permission prompt carries one of the phrases above; the
        // AskUserQuestion decision widget does not — its question is free-form
        // ("How should the connection dialog be split…?"), so the phrase scan
        // misses it and the session wrongly settles to idle. It is still an
        // agent blocked on a human choice, so detect the widget by its own
        // affordances and read it as ApprovalNeeded.
        if is_ask_user_question(screen) {
            return (
                AttentionState::ApprovalNeeded,
                Some("AskUserQuestion prompt awaiting a choice".to_string()),
            );
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
/// ([`AgentPlugin::idle_error`](crate::plugins::AgentPlugin::idle_error), called
/// from the monitor's idle tick) and answers "did it stop *on an error*?".
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

/// True when the rendered screen is Claude Code's **AskUserQuestion** decision
/// widget: a free-form question with a `❯`-selected option menu (the caller has
/// already confirmed the menu) that the agent posts to block on a human choice.
///
/// The generic selection footer ("Enter to select · ↑/↓ to navigate · Esc to
/// cancel") is shared with ordinary pickers (theme, `/model`), so it can't tell
/// them apart. What is *exclusive* to AskUserQuestion is its two extra
/// affordances — the "add notes" hint and the "Chat about this" redirect — which
/// no plain picker or permission prompt renders. Matching either (any-of, so one
/// UI-string rename doesn't silently blind the detector) distinguishes the
/// widget without mistaking a theme picker for a blocked prompt.
fn is_ask_user_question(screen: &str) -> bool {
    let lower = screen.to_lowercase();
    lower.contains("chat about this") || lower.contains("to add notes")
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

    // ---- approval screen matcher ----

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

    /// The captured real-world plan-approval screen from the reported bug: the
    /// tail of the plan is still on screen above a `❯`-selected option menu, but
    /// the question is "Would you like to proceed?" — not the "Do you want to …"
    /// wording every other permission prompt uses — so the phrase scan missed it
    /// and the blocked session read as idle.
    #[test]
    fn claude_plan_approval_screen_is_approval() {
        let screen = "\
   so a supervisor task owns the socket lifecycle:                              \u{2193}\n\
  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
   Claude has written up a plan and is ready to execute. Would you like to proceed?\n\
\n\
   \u{276f} 1. Yes, and bypass permissions\n\
     2. Yes, manually approve edits\n\
     3. No, refine with Ultraplan on Claude Code on the web\n\
     4. Tell Claude what to change\n\
        shift+tab to approve with this feedback\n\
\n\
   ctrl+g to edit in  VS Code  \u{b7} ~/.claude/plans/glittery-honking-lighthouse.md";
        let (a, reason) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
        assert!(reason.unwrap().starts_with("prompt detected: "));
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

    /// The captured real-world AskUserQuestion widget from the reported bug: the
    /// agent posted a free-form decision prompt ("How should the connection
    /// dialog be split…?") and blocked, but with no approval phrase it read as
    /// activity and settled to a calm idle. Its own affordances ("add notes",
    /// "Chat about this") must flag it as blocked.
    #[test]
    fn claude_ask_user_question_is_approval() {
        let screen = "\
 \u{2610} Dialog layout\n\
\n\
How should the connection dialog be split so a long connection list never pushes out the add forms?\n\
\u{276f} 1. Two screens, nested tabs\n\
  2. Flat three tabs\n\
  3. Two panes, one screen\n\
                                  Notes: press n to add notes\n\
  Chat about this\n\
Enter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} n to add notes \u{b7} Esc to cancel";
        let (a, reason) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
        assert!(reason.unwrap().contains("AskUserQuestion"));
    }

    /// A second, unrelated captured AskUserQuestion ("Archive safety") — a
    /// different question and options but the same widget chrome — to pin that
    /// detection keys on the invariant affordances, not this run's wording.
    #[test]
    fn claude_ask_user_question_other_question_is_approval() {
        let screen = "\
 \u{2610} Archive safety\n\
\n\
When archiving a session whose branch has unmerged commits, how should branch removal behave?\n\
  1. Guard, then confirm\n\
\u{276f} 2. Always force-remove\n\
  3. Confirm every archive\n\
                                  Notes: press n to add notes\n\
  Chat about this\n\
Enter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} n to add notes \u{b7} Esc to cancel";
        let (a, _) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
    }

    #[test]
    fn claude_theme_picker_with_nav_footer_is_activity() {
        // A theme/`/model` picker shares the generic selection footer ("Enter to
        // select · ↑/↓ to navigate · Esc to cancel") with AskUserQuestion but has
        // neither the "add notes" nor "Chat about this" affordance, so it must
        // stay activity — the nav footer alone must not read as blocked.
        let screen = " Select theme\n \u{276f} 1. Dark\n   2. Light\n\
                      Enter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} Esc to cancel";
        let (a, _) = claude_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    // ---- stalled-on-error (idle settle) ----

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
