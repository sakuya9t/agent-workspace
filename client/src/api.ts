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
  hostname: string;
  platform: string;
  uptime_ms: number;
  database: string;
  backend: string;
  active_sessions: number;
}

export interface FsEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_git: boolean;
}

export interface FsListing {
  path: string;
  parent: string | null;
  entries: FsEntry[];
}

export interface Workspace {
  id: string;
  name: string;
  root_path: string;
  is_git: boolean;
  created_at: number;
}

export interface WorkspaceInstance {
  id: string;
  workspace_id: string;
  session_id: string | null;
  path: string;
  branch: string | null;
  isolation: string;
  status: string;
  created_at: number;
}

export interface CreateSessionBody {
  agent_plugin_id: string;
  cwd?: string;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  rows?: number;
  cols?: number;
  workspace_id?: string;
  approve_custom?: boolean;
  direct_checkout?: boolean;
}

import { connection } from "./connectionStore";

function apiBase(): string {
  const b = connection().baseUrl;
  return b ? b.replace(/\/$/, "") : "";
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const { token } = connection();
  const headers: Record<string, string> = {
    "content-type": "application/json",
    ...((init?.headers as Record<string, string>) ?? {}),
  };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const res = await fetch(apiBase() + path, { ...init, headers });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const body = await res.json();
      if (body?.error) msg = body.error;
    } catch {
      /* ignore */
    }
    if (res.status === 401) msg = `unauthorized — enroll or reconnect (${msg})`;
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

/** Enroll a device against a specific daemon; returns its device token. */
export async function enrollDevice(
  baseUrl: string,
  enrollmentToken: string,
  deviceName: string,
): Promise<{ server_id: string; device_id: string; device_token: string }> {
  const b = baseUrl ? baseUrl.replace(/\/$/, "") : "";
  const res = await fetch(b + "/api/auth/enroll", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ enrollment_token: enrollmentToken, device_name: deviceName }),
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
  return res.json();
}

/** Probe a daemon's /health (used to validate a connection). */
export async function probeHealth(baseUrl: string, token: string | null): Promise<Health> {
  const b = baseUrl ? baseUrl.replace(/\/$/, "") : "";
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  const res = await fetch(b + "/health", { headers });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
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
  listWorkspaces: () =>
    req<{ workspaces: Workspace[] }>("/api/workspaces").then((r) => r.workspaces),
  addWorkspace: (name: string, root_path: string) =>
    req<{ workspace: Workspace }>("/api/workspaces", {
      method: "POST",
      body: JSON.stringify({ name, root_path }),
    }).then((r) => r.workspace),
  initWorkspaceGit: (id: string) =>
    req<{ workspace: Workspace }>(`/api/workspaces/${id}/init-git`, {
      method: "POST",
    }).then((r) => r.workspace),
  sessionWorkspace: (id: string) =>
    req<{ instance: WorkspaceInstance | null }>(`/api/sessions/${id}/workspace`).then(
      (r) => r.instance,
    ),
  cleanupInstance: (id: string, force: boolean) =>
    req<{ ok: boolean }>(`/api/sessions/${id}/cleanup?force=${force}`, {
      method: "POST",
    }),
  openVscode: (id: string) =>
    req<{ opened: boolean; path: string }>(`/api/sessions/${id}/open-vscode`, {
      method: "POST",
    }),
  fsList: (path: string, showHidden: boolean) =>
    req<FsListing>(
      `/api/fs/list?path=${encodeURIComponent(path)}&show_hidden=${showHidden}`,
    ),
};

export function streamUrl(id: string): string {
  const { baseUrl, token } = connection();
  let host: string;
  let secure: boolean;
  if (baseUrl) {
    const u = new URL(baseUrl);
    host = u.host;
    secure = u.protocol === "https:";
  } else {
    host = location.host;
    secure = location.protocol === "https:";
  }
  let url = `${secure ? "wss" : "ws"}://${host}/api/sessions/${id}/stream`;
  if (token) url += `?access_token=${encodeURIComponent(token)}`;
  return url;
}
