//! Codex's attention classifier.
//!
//! Codex opts into screen-based classification ([`super`] module doc) because
//! its terminal bell is ambiguous: it rings on **turn completion** as well as on
//! approval prompts, so — unlike the shared default heuristic — the bell can't be
//! trusted to mean "blocked". Only the rendered **screen** distinguishes the two.

use crate::domain::AttentionState;

/// Codex's screen-based approval matcher, over the rendered visible **screen**.
///
/// Codex rings the terminal bell on **turn completion** — not only on approval
/// prompts — so the default bell heuristic mis-reads a finished turn as
/// "blocked" and, because that state is sticky, it stays there until the user
/// clicks in. The bell alone therefore can't tell "your turn now" from "I need
/// approval"; only the screen can. So — unlike [`super::default_attention`] — the
/// bell is deliberately ignored here: with no approval menu on screen the turn is
/// simply done, so we report activity and let the idle timer settle it to a calm
/// idle (matching Claude Code, whose finished turns also read as idle).
///
/// Codex renders an approval prompt as a question ("Would you like to run the
/// following command?"), a `›`-selected numbered option menu (`› 1. Yes,
/// proceed`), and a "Press enter to confirm or esc to cancel" footer. The
/// unambiguous "waiting for a choice" signal is the selection pointer on a
/// numbered option; the composer input line also starts with `›` but is never a
/// numbered option, so it isn't mistaken for one. We require *both* the pointer
/// and an approval phrase so an ordinary numbered list Codex printed — or prose
/// that merely quotes such a phrase — isn't read as a prompt.
pub(crate) fn codex_attention(screen: &str, _bell: bool) -> (AttentionState, Option<String>) {
    const APPROVAL_PHRASES: &[&str] = &[
        // The question stem for command / patch approvals.
        "would you like to",
        // The footer on every approval prompt.
        "press enter to confirm",
        // The invariant "No, and tell Codex what to do differently" option.
        "tell codex what to do",
    ];
    if screen.lines().any(is_codex_selected_option) {
        let lower = screen.to_lowercase();
        if let Some(p) = APPROVAL_PHRASES.iter().find(|p| lower.contains(**p)) {
            return (
                AttentionState::ApprovalNeeded,
                Some(format!("prompt detected: {p}")),
            );
        }
    }
    (AttentionState::Activity, None)
}

/// A Codex selection-pointer numbered menu option like "› 1. Yes" or "> 2. No":
/// a `›`/`❯`/`>` pointer (after any indent), then a small integer followed by
/// `.` or `)`. This is what marks Codex's approval menu as *awaiting a choice*,
/// and distinguishes a selected option from the composer input line (which also
/// starts with `›` but carries no numbered option).
fn is_codex_selected_option(line: &str) -> bool {
    let rest = line.trim_start();
    let rest = match ['\u{203a}', '\u{276f}', '>']
        .iter()
        .find_map(|p| rest.strip_prefix(*p))
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

    /// The captured real-world Codex approval prompt: a question, a `›`-selected
    /// numbered option menu, and a "Press enter to confirm" footer. Codex also
    /// rings the bell here, but the screen — not the bell — must decide.
    #[test]
    fn codex_command_approval_screen_is_approval() {
        let screen = "\
 Would you like to run the following command?\n\
   Environment: local\n\
   Reason: Do you want to allow Rust e2e tests to bind loopback ports outside the sandbox?\n\
   $ cargo test --workspace\n\
 \u{203a} 1. Yes, proceed (y)\n\
   2. Yes, and don't ask again for commands that start with `cargo test` (p)\n\
   3. No, and tell Codex what to do differently (esc)\n\
 Press enter to confirm or esc to cancel";
        let (a, reason) = codex_attention(screen, false);
        assert_eq!(a, AttentionState::ApprovalNeeded);
        assert!(reason.unwrap().starts_with("prompt detected: "));
    }

    /// The reported bug: Codex finished a turn and rang the bell, but with no
    /// approval menu on screen it must read as activity (settling to idle later),
    /// NOT blocked. The bell being set is exactly the trap the old default
    /// heuristic fell into.
    #[test]
    fn codex_turn_complete_with_bell_is_activity() {
        let screen = "\
\u{25cf} Committed as ee1d352 \u{2014} Replace button emoji with resource icons.\n\
  - Working tree is clean.\n\
  - No tests required updates.\n\
  - Client build remains unavailable because Node/npm is not installed.\n\
\u{2500} Worked for 10m 19s \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
\u{203a} Write tests for @filename";
        // Even with the turn-completion bell set, this is a finished turn.
        let (a, _) = codex_attention(screen, true);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn codex_composer_line_is_activity() {
        // The composer input line starts with `›` but is not a numbered option,
        // so it must not be mistaken for a selected menu choice.
        let screen = " \u{203a} Yes, proceed with the plan and would you like to keep going";
        let (a, _) = codex_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn codex_numbered_list_without_pointer_is_activity() {
        // A plain numbered list Codex printed (no `›` selection pointer) is
        // output, not an approval menu.
        let screen = "Plan:\n 1. Read\n 2. Edit\n 3. Test";
        let (a, _) = codex_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn codex_prose_quoting_approval_phrase_is_activity() {
        // Prose that merely contains an approval phrase, with no selected option
        // menu, must not read as a prompt.
        let screen = "I finished the edit. Would you like to run the tests next?";
        let (a, _) = codex_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn codex_menu_without_phrase_is_activity() {
        // A `›`-selected numbered menu with no approval phrase is some other
        // picker, not an approval — leave it as activity.
        let screen = " Pick a model:\n \u{203a} 1. gpt-5.5\n   2. gpt-5.5-codex";
        let (a, _) = codex_attention(screen, false);
        assert_eq!(a, AttentionState::Activity);
    }

    #[test]
    fn codex_selected_option_shapes() {
        assert!(is_codex_selected_option(" \u{203a} 1. Yes, proceed (y)"));
        assert!(is_codex_selected_option("\u{276f} 2) No"));
        assert!(is_codex_selected_option("> 3. No"));
        assert!(!is_codex_selected_option(" \u{203a} Write tests for @filename")); // composer
        assert!(!is_codex_selected_option("   2. Yes, and don't ask again")); // not selected
    }
}
