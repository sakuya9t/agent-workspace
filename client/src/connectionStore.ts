import { create } from "zustand";

/**
 * Which daemon the client talks to.
 *
 * - `baseUrl === ""` means same-origin (the daemon serving this page, or the
 *   Vite dev proxy) — the local/loopback case, which needs no token.
 * - A non-empty `baseUrl` is a remote daemon (direct LAN or an SSH-forwarded
 *   localhost port). A `token` is attached when the remote required enrollment.
 */
export interface ConnectionProfile {
  baseUrl: string;
  token: string | null;
  serverId: string | null;
  label: string | null;
}

interface ConnState extends ConnectionProfile {
  setProfile: (p: Partial<ConnectionProfile>) => void;
  reset: () => void;
}

const STORAGE_KEY = "asm.connection";

function load(): ConnectionProfile {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { baseUrl: "", token: null, serverId: null, label: null, ...JSON.parse(raw) };
  } catch {
    /* ignore */
  }
  return { baseUrl: "", token: null, serverId: null, label: null };
}

function persist(p: ConnectionProfile) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(p));
  } catch {
    /* ignore */
  }
}

export const useConnStore = create<ConnState>((set, get) => ({
  ...load(),
  setProfile: (p) => {
    const next = { ...currentProfile(get), ...p };
    persist(next);
    set(next);
  },
  reset: () => {
    const next = { baseUrl: "", token: null, serverId: null, label: null };
    persist(next);
    set(next);
  },
}));

function currentProfile(get: () => ConnState): ConnectionProfile {
  const s = get();
  return { baseUrl: s.baseUrl, token: s.token, serverId: s.serverId, label: s.label };
}

/** Non-reactive accessor for the fetch/WebSocket layer. */
export function connection(): ConnectionProfile {
  const s = useConnStore.getState();
  return { baseUrl: s.baseUrl, token: s.token, serverId: s.serverId, label: s.label };
}
