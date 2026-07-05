import { keepPreviousData, useQueries } from "@tanstack/react-query";
import { api, Health, Session, Workspace } from "./api";
import { DaemonConn, targetOf, useConnStore } from "./connectionStore";

export interface DaemonBundle {
  health: Health;
  sessions: Session[];
  workspaces: Workspace[];
}

export interface DaemonState {
  daemon: DaemonConn;
  data?: DaemonBundle;
  error: unknown;
  isLoading: boolean;
}

/**
 * Polls every connected daemon in parallel and returns their health, sessions,
 * and workspaces. TanStack dedupes by query key, so multiple components calling
 * this share one set of requests per daemon.
 */
export function useDaemonStates(): DaemonState[] {
  const daemons = useConnStore((s) => s.daemons);
  const results = useQueries({
    queries: daemons.map((d) => ({
      queryKey: ["daemon", d.id, d.baseUrl, d.token, d.relayKey ?? null],
      queryFn: async (): Promise<DaemonBundle> => {
        const t = targetOf(d);
        const [health, sessions, workspaces] = await Promise.all([
          api.health(t),
          api.listSessions(t),
          api.listWorkspaces(t),
        ]);
        return { health, sessions, workspaces };
      },
      // A disconnected host stays in the list but is not polled.
      enabled: d.connected,
      refetchInterval: d.connected ? 1500 : (false as const),
      // Keep the last good data during a refetch (and across token/URL changes)
      // so the panel never blanks between polls.
      placeholderData: keepPreviousData,
      // A transient poll failure keeps the previous data (see SessionList: the
      // tree only shows "unreachable" when there is no cached data at all), so a
      // single dropped LAN poll doesn't flash the UI.
      retry: false,
    })),
  });
  return daemons.map((d, i) => ({
    daemon: d,
    data: results[i].data,
    error: results[i].error,
    isLoading: results[i].isLoading,
  }));
}
