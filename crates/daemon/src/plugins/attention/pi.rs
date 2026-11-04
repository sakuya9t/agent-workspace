//! pi's attention classifier.
//!
//! pi has no approval gate to read: it ships **no sandbox and no per-tool
//! permission prompts** (its own `docs/security.md` says so outright), so a
//! running pi session is either working or waiting at a ready prompt. The one
//! thing that genuinely blocks it is the **project-trust** question — asked at
//! startup in a directory carrying `.pi/` settings, extensions, skills or
//! prompts that pi has no saved decision for, and again on demand via `/trust`.
//! Until it is answered, the session sits there having loaded nothing.
//!
//! Neither the shared default heuristic nor the deck can see that question:
//! pi renders its choices as an *unnumbered* list with a `→` pointer, so there
//! is no numbered menu to parse, and none of the default's approval phrases
//! appear in "Trust project folder?".

use crate::domain::AttentionState;

/// pi's screen-based approval matcher, over the rendered visible **screen**.
///
/// The bell is deliberately ignored — and [`bell_means_attention`] left off for
/// this agent — because pi never rings one: every `0x07` it writes terminates an
/// OSC sequence (the OSC 133 prompt marks around each message, OSC 52 for
/// clipboard writes), which is punctuation inside an escape sequence rather than
/// a request for the user.
///
/// Everything that is not the trust question reads as activity, which the
/// monitor's silence timer settles to a calm idle. That is the whole state
/// space: with no approvals to grant, a pi session that has stopped producing
/// output is simply waiting for the next instruction.
///
/// [`bell_means_attention`]: super::super::AgentPlugin::bell_means_attention
pub(crate) fn pi_attention(screen: &str, _bell: bool) -> (AttentionState, Option<String>) {
    // The pointer is what separates a live dialog from pi merely *printing*
    // these words — this repo's own notes on pi's trust model would otherwise
    // read as a prompt the moment the agent quoted them back.
    if !screen.lines().any(is_pi_selected_option) {
        return (AttentionState::Activity, None);
    }
    let lower = screen.to_lowercase();
    // The startup question, then the `/trust` selector, which titles itself
    // differently and always shows the saved decision above its choices.
    let asking = lower.contains("trust project folder?")
        || (lower.contains("project trust") && lower.contains("saved decision:"));
    if asking {
        return (
            AttentionState::ApprovalNeeded,
            Some("prompt detected: project trust".to_string()),
        );
    }
    (AttentionState::Activity, None)
}

/// True when `line` is the selected row of one of pi's option lists: a `→`
/// pointer, then a label. Unselected rows are indented by two spaces instead,
/// so exactly one line of a live dialog matches.
fn is_pi_selected_option(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix('\u{2192}') else {
        return false;
    };
    !rest.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The startup trust question, as pi paints it: title, cwd, explanation, and
    /// the option list with the first row selected.
    const TRUST_PROMPT: &str = "\
Trust project folder?
/home/x/proj

This allows pi to load .pi settings and resources, install missing project packages, and execute project extensions.

\u{2192} Trust
  Trust parent folder (/home/x)
  Trust (this session only)
  Do not trust
  Do not trust (this session only)

\u{2191}\u{2193} navigate  enter select  esc cancel";

    #[test]
    fn trust_prompt_is_approval_needed() {
        let (state, reason) = pi_attention(TRUST_PROMPT, false);
        assert_eq!(state, AttentionState::ApprovalNeeded);
        assert!(reason.unwrap().contains("project trust"));
    }

    #[test]
    fn trust_selector_is_approval_needed() {
        // `/trust` renders its own dialog, which says "Project trust" rather than
        // asking a question, and shows the decision already on file.
        let screen = "\
Project trust
/home/x/proj

Saved decision: none
Current session: untrusted

\u{2192} Trust
  Do not trust";
        assert_eq!(pi_attention(screen, false).0, AttentionState::ApprovalNeeded);
    }

    #[test]
    fn agent_output_quoting_the_prompt_is_activity() {
        // No pointer on screen, so nothing is waiting on a choice — the agent is
        // just talking about project trust.
        let screen = "\
I read docs/security.md. pi asks \"Trust project folder?\" on startup when the
directory carries .pi resources, and stores the answer in trust.json.";
        assert_eq!(pi_attention(screen, false).0, AttentionState::Activity);
    }

    #[test]
    fn an_unrelated_picker_is_activity() {
        // pi uses the same pointer for `/model`, `/resume` and friends. Those are
        // the user driving the TUI, not the agent blocked on them.
        let screen = "Select a model\n\u{2192} anthropic/claude-sonnet-4-5\n  openai/gpt-4o";
        assert_eq!(pi_attention(screen, false).0, AttentionState::Activity);
    }

    #[test]
    fn a_working_turn_is_activity() {
        let (state, reason) = pi_attention("Reading src/auth.ts\u{2026}", false);
        assert_eq!(state, AttentionState::Activity);
        assert!(reason.is_none());
        // pi's OSC terminators are scanned out upstream; even a bell that did
        // reach us must not, on its own, mean "blocked".
        assert_eq!(pi_attention("streaming\u{2026}", true).0, AttentionState::Activity);
    }
}
