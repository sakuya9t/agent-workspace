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

/// Codex's still-working matcher, over the rendered visible **screen**.
///
/// The monitor settles a quiet session to idle after a few seconds of silence,
/// which takes silence to mean "the turn is over, your move". For Codex that
/// reads two *working* states as idle — both captured from a live session:
///
/// * **A turn is still in flight.** Codex shows a status line for as long as it
///   is running — `◦ Working (49s • esc to interrupt)`. Its timer repaints about
///   once a second, so the silence timer rarely gets to fire; but when Codex
///   parks on a long tool call or blocks waiting on a sub-agent (`• Waiting for
///   agents`), a repaint stall longer than the idle delay settles a *running*
///   turn to a calm idle.
/// * **The turn ended but the work it started has not.** Codex leaves a
///   `1 background terminal running · /ps to view · /stop to close` notice up
///   after the turn completes. Nothing streams, so the session goes quiet and
///   reads as idle while the work it kicked off is still going.
///
/// Both markers live in Codex's live status area, not the scrollback: the
/// `Working (…)` line is erased when the turn ends, and the background-terminal
/// notice when the last terminal exits or is stopped. So neither can linger to
/// pin a genuinely finished session as working — and because the settle is
/// retried on each later tick, the session lands on idle as soon as the marker
/// clears.
///
/// The scan is the whole screen rather than the status line alone, because the
/// composer sits *below* the status line and grows with a long typed message,
/// so no fixed offset from the bottom reliably finds it. The cost is that Codex
/// *printing* one of these markers — quoting this very file, say — reads as
/// working until the text scrolls away. That is deliberate: a false "working"
/// self-heals on the next redraw, whereas a false "idle" is the bug this exists
/// to fix, and it strands a session that is genuinely still running.
pub(crate) fn codex_still_working(screen: &str) -> bool {
    // The interrupt affordance is on screen for exactly as long as a turn is in
    // flight, so it — not the animated bullet or the elapsed timer, which both
    // change frame to frame — is the invariant "still running" marker.
    if screen.to_lowercase().contains("esc to interrupt") {
        return true;
    }
    screen.lines().any(has_background_terminals)
}

/// True when `line` carries Codex's outstanding-background-work notice: the
/// `1 background terminal running` status, shown on its own once the turn ends
/// and appended to the `Working (…)` line during one.
///
/// A non-zero count before the phrase and `running` right after it are both
/// required, so prose that merely mentions one ("Starting it in a background
/// terminal now.") isn't read as outstanding work, and neither is Codex's own
/// "No background terminals running." — which reports exactly the opposite.
fn has_background_terminals(line: &str) -> bool {
    const NOTICE: &str = "background terminal";
    let lower = line.to_lowercase();
    let Some(at) = lower.find(NOTICE) else {
        return false;
    };
    let count: String = lower[..at]
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if count.is_empty() || count.chars().all(|c| c == '0') {
        return false;
    }
    lower[at + NOTICE.len()..]
        .trim_start_matches('s') // "terminal" / "terminals"
        .trim_start()
        .starts_with("running")
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

    // ---- still-working (idle settle) ----

    /// The captured real-world screen from the reported bug: Codex was blocked in
    /// `wait_agent`, so it had "nothing to do" itself and the PTY went quiet —
    /// but the turn was very much still in flight. The status line proves it, and
    /// the settle must not demote this to a calm idle.
    #[test]
    fn codex_waiting_for_a_sub_agent_is_still_working() {
        let screen = "\
\u{2022} I\u{2019}ll delegate the command, then wait for the sub-agent to report completion.\n\
\u{2022} Waiting for agents\n\
\u{25e6} Working (49s \u{2022} esc to interrupt) \u{b7} 1 background terminal running \u{b7} /ps to view\n\
\u{203a} Run /review on my current changes\n\
  gpt-5.6-sol xhigh \u{b7} ~/dev/agent-workspace";
        assert!(codex_still_working(screen));
    }

    /// A turn in flight with no sub-agent and no background work: the interrupt
    /// affordance alone is the "still running" marker.
    #[test]
    fn codex_working_status_line_is_still_working() {
        let screen =
            "\u{2022} I\u{2019}ll run it now.\n\u{2022} Working (3s \u{2022} esc to interrupt)\n\u{203a} ";
        assert!(codex_still_working(screen));
    }

    /// The other captured screen from the report: the turn *ended* (no `Working`
    /// line, composer back to its placeholder) but the background terminal it
    /// started is still running — so the session is quiet yet not done, and must
    /// not settle to idle.
    #[test]
    fn codex_background_terminal_outliving_the_turn_is_still_working() {
        let screen = "\
\u{2022} Started it.\n\
\u{2500} Worked for 5m 50s \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
  1 background terminal running \u{b7} /ps to view \u{b7} /stop to close\n\
\u{203a} Run /review on my current changes\n\
  gpt-5.6-sol xhigh \u{b7} ~/dev/agent-workspace";
        assert!(codex_still_working(screen));
    }

    /// The guard on both: the captured *finished*-turn screen, with nothing in
    /// flight and nothing left running. The silence really does mean "your move"
    /// here, so this must still settle to idle — otherwise Codex never hands back.
    #[test]
    fn codex_finished_turn_is_not_still_working() {
        let screen = "\
\u{2022} Ran sleep 25\n\
  \u{2514} (no output)\n\
\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
\u{2022} Command completed successfully.\n\
\u{203a} Run /review on my current changes\n\
  gpt-5.6-sol xhigh \u{b7} ~/dev/agent-workspace";
        assert!(!codex_still_working(screen));
    }

    #[test]
    fn codex_prose_mentioning_a_background_terminal_is_not_still_working() {
        // Codex narrating what it did carries no count and no "running", so it
        // must not read as outstanding work.
        let screen = "\u{2022} Starting it in a background terminal now.\n\u{203a} ";
        assert!(!codex_still_working(screen));
    }

    #[test]
    fn codex_background_terminal_counts() {
        assert!(has_background_terminals("  1 background terminal running \u{b7} /ps to view"));
        assert!(has_background_terminals(
            "\u{25e6} Working (9s \u{2022} esc to interrupt) \u{b7} 2 background terminals running"
        ));
        // `/ps` reporting none outstanding is the opposite of still working.
        assert!(!has_background_terminals("  No background terminals running."));
        assert!(!has_background_terminals("  0 background terminals running"));
        // A count with no "running", and prose, are both not the notice.
        assert!(!has_background_terminals("  1 background terminal stopped"));
        assert!(!has_background_terminals("I opened a background terminal for you"));
    }
}
