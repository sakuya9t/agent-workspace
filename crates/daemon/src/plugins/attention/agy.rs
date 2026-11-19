//! Antigravity CLI (`agy`) attention classifier.
//!
//! agy is the one agent that *tells* the terminal what state it is in. It
//! speaks OSC 7501 — `ESC ] 7501 ; state=<s>[:kind=<k>]:app=antigravity[:msg=<b64>] BEL`
//! — on every transition: `working` when a turn starts or resumes, `blocked`
//! with a `kind` (`permission` for a tool approval or the workspace-trust
//! question, `auth` for a sign-in) while it waits on the user, `done` when a
//! turn finishes, `idle` at a ready prompt, and `failed`. `msg` is base64 text:
//! the pending command or question for a block ("Approve Bash: rm -rf …",
//! "Trust this folder?"), the final reply for `done`.
//!
//! agy only emits it to a terminal that answers its `OSC 7501 ; ?` support probe,
//! which neither xterm.js nor the daemon's emulator does, so the plugin launches
//! it with `AGY_CLI_OSC7501=on` (see `builtin::AgyPlugin`). Both terminals ignore
//! an OSC they don't know, so the status is invisible to the user.
//!
//! That makes this a parser, not a heuristic: there is no prompt wording to
//! match and nothing an agent printing *about* approvals could trip. It reads
//! the raw output **tail** rather than the screen, because an OSC is not drawn.

use crate::domain::AttentionState;

/// The longest reason we carry from a status message into the session list.
const REASON_CHARS: usize = 160;

/// Classify from the newest complete OSC 7501 status in the raw output `tail`.
///
/// A status is sent once per transition, not repeated, so a long stretch of
/// output (a busy turn's spinner) can push it out of the tail. With none left,
/// the chunk is plain **activity**: a block already on record stays blocked
/// through it (the monitor's sticky rule), and silence still settles to idle.
/// The bell is ignored — agy never rings one.
pub(crate) fn agy_attention(tail: &str, _bell: bool) -> (AttentionState, Option<String>) {
    let Some(status) = last_status(tail) else {
        return (AttentionState::Activity, None);
    };
    match status.state.as_str() {
        "working" => (AttentionState::Activity, None),
        "blocked" => {
            let reason = status.message.unwrap_or_else(|| match status.kind.as_deref() {
                Some("auth") => "Antigravity needs you to sign in".to_string(),
                Some(kind) => format!("Antigravity is waiting on you ({kind})"),
                None => "Antigravity is waiting on you".to_string(),
            });
            (AttentionState::ApprovalNeeded, Some(reason))
        }
        // `done` is a finished turn and `idle` a ready prompt — the same thing to
        // the user, who is up next either way. Settled now rather than after the
        // silence timer, because agy has just said so.
        "done" | "idle" => (
            AttentionState::Idle,
            Some("idle — waiting for input".to_string()),
        ),
        "failed" => (
            AttentionState::Error,
            Some(
                status
                    .message
                    .unwrap_or_else(|| "the Antigravity turn failed".to_string()),
            ),
        ),
        // A state this version doesn't know: say nothing stronger than "busy".
        _ => (AttentionState::Activity, None),
    }
}

/// One decoded OSC 7501 status report.
#[derive(Debug, PartialEq)]
struct Status {
    state: String,
    kind: Option<String>,
    /// `msg`, decoded and reduced to one clipped line.
    message: Option<String>,
}

/// The newest *complete* OSC 7501 sequence in `tail`. One still being written
/// (its terminator is in a chunk that hasn't arrived) is skipped in favour of
/// the one before it, and the leftover end of one the tail was trimmed into the
/// middle of has no introducer, so it is never mistaken for a status.
fn last_status(tail: &str) -> Option<Status> {
    const INTRO: &str = "\x1b]7501;";
    let mut found = None;
    let mut rest = tail;
    while let Some(at) = rest.find(INTRO) {
        let body = &rest[at + INTRO.len()..];
        // Terminated by BEL or by ST (`ESC \`).
        let Some(end) = body.find(['\x07', '\x1b']) else {
            break; // incomplete: keep the previous one
        };
        if let Some(status) = parse_status(&body[..end]) {
            found = Some(status);
        }
        rest = &body[end..];
    }
    found
}

/// `state=blocked:kind=permission:app=antigravity:msg=QXBw…` → [`Status`].
/// Fields are `:`-separated `key=value` pairs; base64 never contains a `:`.
/// The `?` agy sends as its support probe has no `state`, so it is not a status.
fn parse_status(body: &str) -> Option<Status> {
    let mut state = None;
    let mut kind = None;
    let mut message = None;
    for field in body.split(':') {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key {
            "state" if !value.is_empty() => state = Some(value.to_string()),
            "kind" if !value.is_empty() => kind = Some(value.to_string()),
            "msg" => message = decode_base64(value).and_then(|bytes| one_line(&bytes)),
            _ => {}
        }
    }
    Some(Status {
        state: state?,
        kind,
        message,
    })
}

/// The message as a single clipped line: a `done` carries the whole final reply,
/// which may run to paragraphs of Markdown.
fn one_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    if line.chars().count() <= REASON_CHARS {
        return Some(line.to_string());
    }
    let clipped: String = line.chars().take(REASON_CHARS).collect();
    Some(format!("{}…", clipped.trim_end()))
}

/// Standard or URL-safe base64, padding optional. `None` on any other byte.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.trim_end_matches('=').bytes() {
        acc = (acc << 6) | value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osc(body: &str) -> String {
        format!("\x1b]7501;{body}\x07")
    }

    #[test]
    fn a_permission_prompt_is_approval_needed_with_agys_own_reason() {
        // Captured from agy 1.5.10 asking to run a shell command.
        let tail = format!(
            "{}redraw{}",
            osc("state=working:app=antigravity"),
            osc("state=blocked:kind=permission:app=antigravity:msg=QXBwcm92ZSBCYXNoOiBybSAtcmYgLi9qdW5rMiAmJiBlY2hvIHJlbW92ZWQyID4gc3RhdHVzMi50eHQ="),
        );
        assert_eq!(
            agy_attention(&tail, false),
            (
                AttentionState::ApprovalNeeded,
                Some("Approve Bash: rm -rf ./junk2 && echo removed2 > status2.txt".to_string())
            )
        );
    }

    #[test]
    fn the_workspace_trust_question_blocks_too() {
        let tail = osc("state=blocked:kind=permission:app=antigravity:msg=VHJ1c3QgdGhpcyBmb2xkZXI/");
        assert_eq!(
            agy_attention(&tail, false),
            (AttentionState::ApprovalNeeded, Some("Trust this folder?".to_string()))
        );
    }

    #[test]
    fn the_newest_status_wins() {
        // Answering the prompt: agy reports `working` right after the block.
        let tail = format!(
            "{}{}",
            osc("state=blocked:kind=permission:app=antigravity:msg=eA=="),
            osc("state=working:app=antigravity"),
        );
        assert_eq!(agy_attention(&tail, false), (AttentionState::Activity, None));
        // A finished turn settles straight to idle, without the silence timer.
        let tail = format!(
            "{}{}",
            osc("state=working:app=antigravity"),
            osc("state=done:app=antigravity:msg=RG9uZS4KCk1vcmUgcHJvc2Uu"),
        );
        assert_eq!(agy_attention(&tail, false).0, AttentionState::Idle);
    }

    #[test]
    fn failure_is_an_error_not_a_dead_session() {
        let tail = osc("state=failed:app=antigravity:msg=UXVvdGEgZXhjZWVkZWQ=");
        assert_eq!(
            agy_attention(&tail, false),
            (AttentionState::Error, Some("Quota exceeded".to_string()))
        );
    }

    #[test]
    fn a_block_without_a_message_names_its_kind() {
        // `AGY_CLI_OSC7501_MESSAGES=brief` drops `msg`.
        let tail = osc("state=blocked:kind=auth:app=antigravity");
        let (state, reason) = agy_attention(&tail, false);
        assert_eq!(state, AttentionState::ApprovalNeeded);
        assert!(reason.unwrap().contains("sign in"));
    }

    #[test]
    fn output_with_no_status_left_in_the_tail_is_activity() {
        // A spinner long enough to have pushed the last status out — or prose
        // that merely *talks* about approvals, which never reads as a prompt.
        let tail = "⡿ Generating... Do you want to proceed? (y/n) Run this command?";
        assert_eq!(agy_attention(tail, true), (AttentionState::Activity, None));
    }

    #[test]
    fn an_unfinished_or_trimmed_sequence_is_not_read() {
        // The terminator hasn't arrived yet: the previous status still stands.
        let tail = format!(
            "{}\x1b]7501;state=work",
            osc("state=blocked:kind=permission:app=antigravity:msg=eA==")
        );
        assert_eq!(agy_attention(&tail, false).0, AttentionState::ApprovalNeeded);
        // The tail was cut inside one: no introducer, so nothing to read.
        let tail = "ate=blocked:kind=permission:app=antigravity\x07spinner";
        assert_eq!(agy_attention(tail, false).0, AttentionState::Activity);
        // String-terminated (`ESC \`) is the same sequence.
        let tail = "\x1b]7501;state=idle:app=antigravity\x1b\\";
        assert_eq!(agy_attention(tail, false).0, AttentionState::Idle);
    }

    #[test]
    fn the_support_probe_and_unknown_states_change_nothing() {
        assert_eq!(agy_attention(&osc("?"), false), (AttentionState::Activity, None));
        let tail = osc("state=pondering:app=antigravity");
        assert_eq!(agy_attention(&tail, false), (AttentionState::Activity, None));
    }

    #[test]
    fn long_messages_become_one_clipped_line() {
        let long = format!("{}\nsecond line", "word ".repeat(60));
        let reason = one_line(long.as_bytes()).unwrap();
        assert!(reason.ends_with('…'));
        assert!(reason.chars().count() <= REASON_CHARS + 1);
        assert!(!reason.contains("second line"));
    }

    #[test]
    fn base64_decodes_with_or_without_padding() {
        assert_eq!(decode_base64("aGk=").unwrap(), b"hi");
        assert_eq!(decode_base64("aGk").unwrap(), b"hi");
        assert_eq!(decode_base64("").unwrap(), b"");
        assert_eq!(decode_base64("a$b"), None);
    }
}
