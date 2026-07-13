import { create } from "zustand";
import i18n from "./i18n";

/**
 * A daemon the client talks to. The client can hold several at once and
 * aggregates their sessions in the left panel.
 *
 * - `baseUrl === ""` is same-origin (the daemon serving this page / the Vite
 *   dev proxy). Same-origin is NOT the same as loopback: the daemon serves the
 *   client itself, so a phone on the LAN opening `https://<host>:4600` is
 *   same-origin too, and the daemon only waives the token for loopback peers.
 *   So the local entry carries a `token` like any other once enrolled.
 * - A non-empty `baseUrl` is a remote daemon: direct LAN, an SSH-forwarded
 *   localhost port, or one reached through a relay (`baseUrl` then ends in
 *   `/n/<node_id>` and `relayKey`/`via` are set). `token` is set when the
 *   remote required enrollment.
 */
export interface DaemonConn {
  id: string;
  baseUrl: string;
  token: string | null;
  label: string;
  /** When false, the client keeps the host in the list but stops polling it. */
  connected: boolean;
  /** Relay access key, when this daemon is reached through a relay. */
  relayKey?: string | null;
  /** Id of the relay this daemon is reached through (grouping / failure state). */
  via?: string;
}

/**
 * A relay the client can reach nodes through. The relay routes `/n/<node_id>`
 * over its multiplexed connections; the client only ever speaks plain
 * HTTP(S)/WSS to it — no tunnels, so it works from any client, including mobile.
 */
export interface RelayConn {
  id: string;
  /** Base URL with scheme + authority, no path, no trailing slash. */
  url: string;
  accessKey: string;
  label: string;
}

export interface Target {
  baseUrl: string;
  token: string | null;
  /** Relay access key, added as a header/param when reaching a relayed node. */
  relayKey?: string | null;
}

export const targetOf = (d: DaemonConn): Target => ({
  baseUrl: d.baseUrl,
  token: d.token,
  relayKey: d.relayKey ?? null,
});

/**
 * Display label for a daemon. The local daemon's stored label is an internal
 * sentinel — persisted data is never localized, so its visible text resolves
 * through i18n at render time. User-entered remote labels are data, shown as-is.
 */
export function daemonLabel(d: DaemonConn): string {
  return d.id === "local" ? i18n.t("common.thisMachine") : d.label;
}

interface ConnState {
  daemons: DaemonConn[];
  relays: RelayConn[];
  addDaemon: (d: Omit<DaemonConn, "id" | "connected">) => string;
  updateDaemon: (id: string, patch: Partial<DaemonConn>) => void;
  removeDaemon: (id: string) => void;
  addRelay: (r: Omit<RelayConn, "id">) => string;
  removeRelay: (id: string) => void;
}

const STORAGE_KEY = "asm.daemons";
const RELAYS_KEY = "asm.relays";
const LOCAL: DaemonConn = {
  id: "local",
  baseUrl: "",
  token: null,
  label: "This machine",
  connected: true,
};

function newId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return "d-" + Math.random().toString(36).slice(2);
  }
}

function load(): DaemonConn[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const arr = JSON.parse(raw) as DaemonConn[];
      if (Array.isArray(arr) && arr.length) {
        // Ensure the local daemon is always present and first, preserving its
        // connected state and device token; default older entries (no
        // `connected`) to true. The token matters off-loopback: a phone opening
        // the daemon-served UI over the LAN is same-origin but still has to
        // enroll, and dropping the token here would re-401 it on every reload.
        const persistedLocal = arr.find((d) => d.id === "local");
        const local: DaemonConn = {
          ...LOCAL,
          token: persistedLocal?.token ?? null,
          connected: persistedLocal?.connected ?? true,
        };
        const others = arr
          .filter((d) => d.id !== "local" && d.baseUrl !== "")
          .map((d) => ({ ...d, connected: d.connected ?? true }));
        return [local, ...others];
      }
    }
    // Migrate a legacy single-connection profile, if any.
    const legacy = localStorage.getItem("asm.connection");
    if (legacy) {
      const p = JSON.parse(legacy);
      if (p?.baseUrl) {
        return [
          LOCAL,
          {
            id: newId(),
            baseUrl: p.baseUrl,
            token: p.token ?? null,
            label: p.label ?? p.baseUrl,
            connected: true,
          },
        ];
      }
    }
  } catch {
    /* ignore */
  }
  return [LOCAL];
}

function loadRelays(): RelayConn[] {
  try {
    const raw = localStorage.getItem(RELAYS_KEY);
    if (raw) {
      const arr = JSON.parse(raw) as RelayConn[];
      if (Array.isArray(arr)) return arr.filter((r) => r && r.url);
    }
  } catch {
    /* ignore */
  }
  return [];
}

function persist(daemons: DaemonConn[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(daemons));
  } catch {
    /* ignore */
  }
}

function persistRelays(relays: RelayConn[]) {
  try {
    localStorage.setItem(RELAYS_KEY, JSON.stringify(relays));
  } catch {
    /* ignore */
  }
}

export const useConnStore = create<ConnState>((set, get) => ({
  daemons: load(),
  relays: loadRelays(),
  addDaemon: (d) => {
    const id = newId();
    const next = [...get().daemons, { ...d, id, connected: true }];
    persist(next);
    set({ daemons: next });
    return id;
  },
  updateDaemon: (id, patch) => {
    const next = get().daemons.map((d) => (d.id === id ? { ...d, ...patch } : d));
    persist(next);
    set({ daemons: next });
  },
  removeDaemon: (id) => {
    if (id === "local") return; // local is always available
    const next = get().daemons.filter((d) => d.id !== id);
    persist(next);
    set({ daemons: next });
  },
  addRelay: (r) => {
    const id = newId();
    const next = [...get().relays, { ...r, id }];
    persistRelays(next);
    set({ relays: next });
    return id;
  },
  removeRelay: (id) => {
    const relays = get().relays.filter((r) => r.id !== id);
    // Cascade: drop any daemons reached through this relay.
    const daemons = get().daemons.filter((d) => d.via !== id);
    persistRelays(relays);
    persist(daemons);
    set({ relays, daemons });
  },
}));

/** Non-reactive accessor to the local daemon target (for one-off calls). */
export function localTarget(): Target {
  const local = useConnStore.getState().daemons.find((d) => d.id === "local");
  return { baseUrl: "", token: local?.token ?? null };
}

/**
 * Is this page served from a loopback origin? Only then does the daemon trust
 * the peer without a token (see ASM_TRUST_LOOPBACK). The same bundle served to
 * a phone over the LAN is same-origin but off-loopback, so it must enroll.
 */
export function isLoopbackOrigin(): boolean {
  const h = location.hostname;
  return h === "localhost" || h === "127.0.0.1" || h === "::1" || h === "[::1]";
}
