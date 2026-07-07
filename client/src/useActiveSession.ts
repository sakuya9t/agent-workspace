import { ActiveRef, useUiStore } from "./store";
import { Target, targetOf } from "./connectionStore";
import { DaemonState, useDaemonStates } from "./useDaemons";
import { Session } from "./api";
import { isLive } from "./status";

/** Everything a shell needs to render the fleet and the selected session. */
export interface ActiveSession {
  /** All polled daemon states (shared query — reading this here dedupes). */
  states: DaemonState[];
  /** Connected daemons that have returned data (health dot / summary). */
  reachable: number;
  /** Live sessions across all connected daemons (header count). */
  totalLive: number;
  /** The current selection ref, straight from the store. */
  active: ActiveRef | null;
  /** The daemon state owning the active session, if any. */
  activeState: DaemonState | undefined;
  /** The selected session record, if it still exists in the poll. */
  activeSession: Session | undefined;
  /** Connection target for the active daemon (undefined if none selected). */
  target: Target | undefined;
  /** Whether the active session accepts terminal input. */
  live: boolean;
}

/**
 * Polls every connected daemon and derives the active-session view: the daemon
 * fleet, health counts, and the selected session with its connection target and
 * live flag. Extracted from App.tsx so the desktop and (upcoming) mobile shells
 * share one wiring instead of duplicating the poll + derivation verbatim.
 */
export function useActiveSession(): ActiveSession {
  const active = useUiStore((s) => s.activeSession);
  const states = useDaemonStates();

  const reachable = states.filter((s) => s.daemon.connected && s.data).length;
  const totalLive = states.reduce(
    (n, s) =>
      n +
      (s.daemon.connected
        ? (s.data?.sessions.filter((x) => isLive(x.status)).length ?? 0)
        : 0),
    0,
  );

  const activeState = active
    ? states.find((s) => s.daemon.id === active.daemonId)
    : undefined;
  const activeSession = activeState?.data?.sessions.find(
    (s) => s.id === active?.sessionId,
  );
  const target = activeState ? targetOf(activeState.daemon) : undefined;
  const live = activeSession ? isLive(activeSession.status) : false;

  return { states, reachable, totalLive, active, activeState, activeSession, target, live };
}
