// Typed client for the daemon HTTP API. Paths are proxied by Vite in dev.

export type SessionStatus =
  | "starting"
  | "running"
  | "exited"
  | "failed"
  | "stopped"
  | "archived";

export type AttentionState =
  | "none"
  | "activity"
  | "likely_blocked"
  | "approval_needed"
  | "failed";

export interface Session {
  id: string;
  agent_plugin_id: string;
  command: string;
  args: string[];
  env: [string, string][];
  working_directory: string;
  workspace_id: string | null;
  status: SessionStatus;
  rows: number;
  cols: number;
  last_event_seq: number;
  exit_code: number | null;
  attention_state: AttentionState;
  attention_reason: string | null;
  created_at: number;
  updated_at: number;
  last_activity_at: number;
}

export interface PluginInfo {
  id: string;
  display_name: string;
  supported_platforms: string[];
  available: boolean;
  binary_path: string | null;
  supported_on_this_platform: boolean;
}

export interface SessionSummary {
  id: string;
  session_id: string;
  agent_plugin_id: string;
  started_at: number;
  ended_at: number;
  duration_ms: number;
  exit_status: string;
  terminal_event_start: number;
  terminal_event_end: number;
}

export interface ChangedFile {
  path: string;
  status: string;
  staged: boolean;
  untracked: boolean;
  orig_path: string | null;
}

export interface ScmStatus {
  is_repo: boolean;
  provider: string;
  branch: string | null;
  head: string | null;
  detached: boolean;
  changed_files: ChangedFile[];
}

export interface Commit {
  hash: string;
  short: string;
  subject: string;
  author: string;
  timestamp: number;
  parents: string[];
}

export interface Health {
  status: string;
  version: string;
  platform: string;
  uptime_ms: number;
  database: string;
  backend: string;
  active_sessions: number;
}

export interface CreateSessionBody {
  agent_plugin_id: string;
  cwd: string;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  rows?: number;
  cols?: number;
  approve_custom?: boolean;
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const body = await res.json();
      if (body?.error) msg = body.error;
    } catch {
      /* ignore */
    }
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

export const api = {
  health: () => req<Health>("/health"),
  listPlugins: () => req<{ plugins: PluginInfo[] }>("/api/plugins").then((r) => r.plugins),
  listSessions: () =>
    req<{ sessions: Session[] }>("/api/sessions").then((r) => r.sessions),
  getSession: (id: string) =>
    req<{ session: Session }>(`/api/sessions/${id}`).then((r) => r.session),
  getSummary: (id: string) =>
    req<{ summary: SessionSummary }>(`/api/sessions/${id}/summary`).then((r) => r.summary),
  createSession: (body: CreateSessionBody) =>
    req<{ session: Session }>("/api/sessions", {
      method: "POST",
      body: JSON.stringify(body),
    }).then((r) => r.session),
  stopSession: (id: string) =>
    req<{ session: Session }>(`/api/sessions/${id}/stop`, { method: "POST" }).then(
      (r) => r.session,
    ),
  archiveSession: (id: string) =>
    req<{ session: Session }>(`/api/sessions/${id}/archive`, { method: "POST" }).then(
      (r) => r.session,
    ),
  ackAttention: (id: string) =>
    req<{ session: Session }>(`/api/sessions/${id}/ack`, { method: "POST" }).then(
      (r) => r.session,
    ),
  resizeSession: (id: string, rows: number, cols: number) =>
    req<{ ok: boolean }>(`/api/sessions/${id}/resize`, {
      method: "POST",
      body: JSON.stringify({ rows, cols }),
    }),
  scmStatus: (id: string) =>
    req<{ status: ScmStatus }>(`/api/sessions/${id}/scm/status`).then((r) => r.status),
  scmDiff: (id: string, path: string, untracked: boolean) =>
    req<{ path: string; diff: string }>(
      `/api/sessions/${id}/scm/diff?path=${encodeURIComponent(path)}&untracked=${untracked}`,
    ).then((r) => r.diff),
  scmLog: (id: string, limit = 30) =>
    req<{ commits: Commit[] }>(`/api/sessions/${id}/scm/log?limit=${limit}`).then(
      (r) => r.commits,
    ),
};

export function streamUrl(id: string): string {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  return `${proto}://${location.host}/api/sessions/${id}/stream`;
}
