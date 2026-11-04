import { AttentionState, SessionStatus } from "./api";

// Single source of truth for session-status semantics. Three copies of `isLive`
// used to drift across App/SessionList/RightPanel (the last as an inverse
// "terminal" list); this module is the one place the meaning is defined.
//
// The seven statuses split three ways, not two: `indeterminate` is deliberately
// **neither** live nor terminal. It marks a session the daemon lost track of
// across a restart/reconnect — the backing process may still be alive, so we
// can't treat it as running (no input, no stop button) but we also can't treat
// it as finished (no ended-summary, no worktree cleanup) until adoption
// resolves it back to a live or terminal status.

/** Actively attached, or coming up. Accepts terminal input. */
export function isLive(status: SessionStatus): boolean {
  return status === "running" || status === "starting";
}

/**
 * Definitively ended — the process is gone and will not resume, so the
 * ended-summary and worktree-cleanup affordances apply. `indeterminate` is
 * excluded on purpose (see module note): it is unresolved, not ended.
 */
export function isTerminal(status: SessionStatus): boolean {
  return (
    status === "exited" ||
    status === "failed" ||
    status === "stopped" ||
    status === "archived"
  );
}

/**
 * The agent is waiting on the user: it likely hit a blocking prompt, is asking
 * for approval (both render as "blocked"), or stopped on an error mid-turn
 * ("error" — e.g. an API failure killed the turn and only a retry resumes it).
 * This is the signal that a session "needs attention" — the states are grouped
 * here so the tab alert, badges, and any future notifications all agree on
 * what counts.
 */
export function needsAttention(attention: AttentionState): boolean {
  return (
    attention === "likely_blocked" ||
    attention === "approval_needed" ||
    attention === "error"
  );
}

/**
 * The agent has a turn in flight: it is working ("activity"), or blocked on a
 * prompt waiting for the user to let that turn proceed. Stopping here throws
 * the work away, so the daemon refuses an unforced stop for these states and
 * the stop dialog gates the button behind a "force stop" checkbox.
 *
 * Only working and blocked are protected. "idle", "none" (silent — no signal,
 * e.g. a plain shell, which opts out of attention tracking entirely) and
 * "error" all stop in one click. That makes this narrower than
 * `needsAttention`, which counts "error": needing attention is not the same as
 * being mid-turn — an errored turn has already aborted, so there is nothing
 * left to lose. This is the client-side twin of `AttentionState::is_busy`
 * (`crates/daemon/src/domain.rs`); the two definitions have to agree.
 */
export function isBusy(attention: AttentionState): boolean {
  return (
    attention === "activity" ||
    attention === "likely_blocked" ||
    attention === "approval_needed"
  );
}
