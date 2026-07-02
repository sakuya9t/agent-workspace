import { create } from "zustand";

/**
 * A daemon the client talks to. The client can hold several at once and
 * aggregates their sessions in the left panel.
 *
 * - `baseUrl === ""` is same-origin (the daemon serving this page / the Vite
 *   dev proxy) — the local/loopback case, no token needed.
 * - A non-empty `baseUrl` is a remote daemon (direct LAN or an SSH-forwarded
 *   localhost port); `token` is set when the remote required enrollment.
 */
export interface DaemonConn {
  id: string;
  baseUrl: string;
  token: string | null;
  label: string;
}

export interface Target {
  baseUrl: string;
  token: string | null;
}

export const targetOf = (d: DaemonConn): Target => ({
  baseUrl: d.baseUrl,
  token: d.token,
});

interface ConnState {
  daemons: DaemonConn[];
  addDaemon: (d: Omit<DaemonConn, "id">) => string;
  updateDaemon: (id: string, patch: Partial<DaemonConn>) => void;
  removeDaemon: (id: string) => void;
}

const STORAGE_KEY = "asm.daemons";
const LOCAL: DaemonConn = { id: "local", baseUrl: "", token: null, label: "This machine" };

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
        // Ensure the local daemon is always present and first.
        const withoutLocal = arr.filter((d) => d.id !== "local" && d.baseUrl !== "");
        return [LOCAL, ...withoutLocal];
      }
    }
    // Migrate a legacy single-connection profile, if any.
    const legacy = localStorage.getItem("asm.connection");
    if (legacy) {
      const p = JSON.parse(legacy);
      if (p?.baseUrl) {
        return [LOCAL, { id: newId(), baseUrl: p.baseUrl, token: p.token ?? null, label: p.label ?? p.baseUrl }];
      }
    }
  } catch {
    /* ignore */
  }
  return [LOCAL];
}

function persist(daemons: DaemonConn[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(daemons));
  } catch {
    /* ignore */
  }
}

export const useConnStore = create<ConnState>((set, get) => ({
  daemons: load(),
  addDaemon: (d) => {
    const id = newId();
    const next = [...get().daemons, { ...d, id }];
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
}));

/** Non-reactive accessor to the local daemon target (for one-off calls). */
export function localTarget(): Target {
  return { baseUrl: "", token: null };
}
