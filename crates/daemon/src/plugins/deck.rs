//! Structured, button-addressable view of an agent's terminal approval menu.
//!
//! The browser deck and a future physical Stream Deck both consume this model.
//! Keeping terminal parsing and key generation in the daemon means controllers
//! never need to understand ANSI, provider-specific screens, or PTY modes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::Serialize;

/// One choice shown in a blocked agent's numbered terminal menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeckOption {
    /// Stable within this prompt. This is the number rendered by the agent.
    pub id: usize,
    pub label: String,
}

/// A terminal prompt reduced to the information a button controller needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeckPrompt {
    /// Changes when the question, context, choices, or current selection changes.
    /// Controllers echo it on response so a tap cannot answer a newer prompt.
    pub revision: String,
    pub question: String,
    /// Command/reason/context rendered between the question and choices.
    pub detail: String,
    pub options: Vec<DeckOption>,
    /// Option id carrying the terminal's selection pointer right now.
    pub selected: usize,
}

#[derive(Debug, Clone)]
struct MenuLine {
    line: usize,
    id: usize,
    label: String,
    selected: bool,
}

/// Parse the numbered menu used by Codex and Claude Code approval/decision UIs.
/// Returns `None` for prose, ordinary numbered lists, and unselected pickers.
pub fn parse_prompt(screen: &str) -> Option<DeckPrompt> {
    let lines: Vec<&str> = screen.lines().collect();
    let numbered: Vec<MenuLine> = lines
        .iter()
        .enumerate()
        .filter_map(|(line, text)| parse_menu_line(line, text))
        .collect();
    let selected_at = numbered.iter().position(|m| m.selected)?;

    // Grow the consecutive option run around the selected row. A wrapped label
    // may leave a few physical lines between numbered rows; a separate numbered
    // list elsewhere on the screen must not leak into the approval choices.
    let mut start = selected_at;
    while start > 0 {
        let prev = &numbered[start - 1];
        let cur = &numbered[start];
        if prev.id + 1 == cur.id && cur.line.saturating_sub(prev.line) <= 4 {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = selected_at + 1;
    while end < numbered.len() {
        let prev = &numbered[end - 1];
        let cur = &numbered[end];
        if prev.id + 1 == cur.id && cur.line.saturating_sub(prev.line) <= 4 {
            end += 1;
        } else {
            break;
        }
    }
    let menu = &numbered[start..end];
    let selected = numbered[selected_at].id;
    let first_line = menu.first()?.line;

    // Approval context is intentionally bounded to the visible lines nearest the
    // menu. It captures Codex's Environment / Reason / `$ command` block and
    // Claude's AskUserQuestion text without hauling transcript history into a
    // tiny controller screen.
    let context_start = first_line.saturating_sub(12);
    let context: Vec<(usize, String)> = lines[context_start..first_line]
        .iter()
        .enumerate()
        .filter_map(|(offset, raw)| {
            let cleaned = clean_context_line(raw)?;
            Some((context_start + offset, cleaned))
        })
        .collect();
    let question_at = context
        .iter()
        .position(|(_, text)| is_primary_question(text))
        .or_else(|| context.iter().rposition(|(_, text)| text.ends_with('?')))
        .or_else(|| {
            context
                .iter()
                .position(|(_, text)| text.to_lowercase().contains("requires approval"))
        })
        .unwrap_or_else(|| context.len().saturating_sub(1));
    let question = context
        .get(question_at)
        .map(|(_, text)| text.clone())
        .unwrap_or_else(|| "Approval requested".to_string());
    let question_line = context.get(question_at).map(|(line, _)| *line);
    let detail = context
        .iter()
        .filter(|(line, text)| Some(*line) != question_line && text != &question)
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let options: Vec<DeckOption> = menu
        .iter()
        .map(|m| DeckOption {
            id: m.id,
            label: m.label.clone(),
        })
        .collect();

    let mut hasher = DefaultHasher::new();
    question.hash(&mut hasher);
    detail.hash(&mut hasher);
    selected.hash(&mut hasher);
    for option in &options {
        option.id.hash(&mut hasher);
        option.label.hash(&mut hasher);
    }

    Some(DeckPrompt {
        revision: format!("{:016x}", hasher.finish()),
        question,
        detail,
        options,
        selected,
    })
}

fn parse_menu_line(line: usize, text: &str) -> Option<MenuLine> {
    let mut rest = text.trim_start();
    let mut selected = false;
    if let Some(after) = ['\u{203a}', '\u{276f}', '>']
        .iter()
        .find_map(|pointer| rest.strip_prefix(*pointer))
    {
        selected = true;
        rest = after.trim_start();
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let id = rest[..digits].parse().ok()?;
    rest = &rest[digits..];
    rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    let label = rest.trim().to_string();
    if label.is_empty() {
        return None;
    }
    Some(MenuLine {
        line,
        id,
        label,
        selected,
    })
}

fn clean_context_line(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty()
        || text.chars().all(|c| ('\u{2500}'..='\u{257f}').contains(&c))
        || text.to_lowercase().starts_with("press enter to confirm")
        || text.to_lowercase().starts_with("esc to cancel")
    {
        return None;
    }
    Some(text.to_string())
}

fn is_primary_question(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("would you like to") || lower.starts_with("do you want to proceed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_approval_with_reason_and_command() {
        let screen = "\
 Would you like to run the following command?\n\
   Environment: local\n\
   Reason: Run the complete workspace test suite.\n\
   $ cargo test --workspace\n\
 \u{203a} 1. Yes, proceed (y)\n\
   2. Yes, and don't ask again for `cargo test` (p)\n\
   3. No, and tell Codex what to do differently (esc)\n\
 Press enter to confirm or esc to cancel";
        let prompt = parse_prompt(screen).unwrap();
        assert_eq!(
            prompt.question,
            "Would you like to run the following command?"
        );
        assert!(prompt.detail.contains("$ cargo test --workspace"));
        assert_eq!(prompt.selected, 1);
        assert_eq!(prompt.options.len(), 3);
        assert!(prompt.options[1].label.contains("don't ask again"));
    }

    #[test]
    fn parses_claude_question_with_non_first_selection() {
        let screen = "\
 Archive safety\n\
 When archiving a session, what should branch removal do?\n\
   1. Guard, then confirm\n\
 \u{276f} 2. Always force-remove\n\
   3. Confirm every archive\n\
 Notes: press n to add notes";
        let prompt = parse_prompt(screen).unwrap();
        assert_eq!(
            prompt.question,
            "When archiving a session, what should branch removal do?"
        );
        assert_eq!(prompt.selected, 2);
        assert_eq!(
            prompt.options.iter().map(|o| o.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn prefers_the_question_over_an_approval_heading() {
        let screen = "\
 This command requires approval\n\
 Do you want to proceed?\n\
 \u{276f} 1. Yes\n\
   2. No";
        let prompt = parse_prompt(screen).unwrap();
        assert_eq!(prompt.question, "Do you want to proceed?");
        assert!(prompt.detail.contains("This command requires approval"));
    }

    #[test]
    fn rejects_plain_numbered_output_without_selection_pointer() {
        assert!(parse_prompt("Plan:\n 1. Read\n 2. Edit\n 3. Test").is_none());
    }

    #[test]
    fn revision_changes_with_selection() {
        let a = parse_prompt("Proceed?\n\u{203a} 1. Yes\n  2. No").unwrap();
        let b = parse_prompt("Proceed?\n  1. Yes\n\u{203a} 2. No").unwrap();
        assert_ne!(a.revision, b.revision);
    }
}
