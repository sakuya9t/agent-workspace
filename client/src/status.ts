import { SessionStatus } from "./api";

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
