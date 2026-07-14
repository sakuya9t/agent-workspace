// Typed client for the daemon HTTP API. Paths are proxied by Vite in dev.

export type SessionStatus =
  | "starting"
  | "running"
  | "exited"
  | "failed"
  | "stopped"
  | "archived"
  | "indeterminate";

export type AttentionState =
  | "none"
  | "activity"
  | "idle"
  | "likely_blocked"
  | "approval_needed"
  | "error"
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
  risky: boolean;
  /** Whether a live client is currently attached (for takeover prompts). */
  attached?: boolean;
  /** The agent's own name for the session (best-effort; absent on old daemons). */
  title?: string | null;
  /** Branch held by the session's workspace instance, for the info popover. */
  branch?: string | null;
  /** Origin session id when this session was forked from another; null otherwise. */
  forked_from?: string | null;
  /**
   * Whether the agent kept a conversation the daemon could resume. A fork onto
   * the same agent then carries the whole conversation; otherwise it carries a
   * written summary. (Absent on old daemons.)
   */
  has_agent_conversation?: boolean;
}

export interface AgentOption {
  key: string;
  label: string;
  description: string;
  danger: boolean;
  default: boolean;
}

export interface PluginInfo {
  id: string;
  display_name: string;
  supported_platforms: string[];
  available: boolean;
  binary_path: string | null;
  supported_on_this_platform: boolean;
  options: AgentOption[];
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

export interface RateLimitWindow {
  label: string;
  used_percent: number;
  window_minutes: number | null;
  /** Unix seconds at which the window resets. */
  resets_at: number | null;
}

/** Best-effort per-session usage, read from the agent's on-disk transcript. */
export interface SessionUsage {
  available: boolean;
  source: string | null;
  model: string | null;
  context_tokens: number | null;
  context_window: number | null;
  input_tokens: number | null;
  cached_input_tokens: number | null;
  output_tokens: number | null;
  reasoning_tokens: number | null;
  total_tokens: number | null;
  rate_limits: RateLimitWindow[];
  updated_at: string | null;
  note: string | null;
}

export interface ChangedFile {
  path: string;
  status: string;
  staged: boolean;
  untracked: boolean;
  orig_path: string | null;
}

/** One remote's tip for the current branch, as of the last fetch. */
export interface RemoteBranch {
  remote: string;
  branch: string;
  head: string;
  ahead: number;
  behind: number;
  upstream: boolean;
}

/** The commit the current branch was spawned at, or last rebased onto. */
export interface BaseCommit {
  hash: string;
  short: string;
  subject: string;
  kind: "spawned" | "rebased";
  refs: string[];
  ahead: number;
}

export interface ScmStatus {
  is_repo: boolean;
  provider: string;
  branch: string | null;
  head: string | null;
  detached: boolean;
  changed_files: ChangedFile[];
  remotes: RemoteBranch[];
  base: BaseCommit | null;
}

export interface Commit {
  hash: string;
  short: string;
  subject: string;
  author: string;
  timestamp: number;
  parents: string[];
}

export interface CommitFileStat {
  path: string;
  orig_path: string | null;
  additions: number | null;
  deletions: number | null;
}

export interface CommitDetail {
  hash: string;
  short: string;
  subject: string;
  body: string;
  author: string;
  email: string;
  timestamp: number;
  parents: string[];
  files: CommitFileStat[];
  additions: number;
  deletions: number;
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
  /** Whether root_path currently exists on the host (from the list endpoint). */
  root_exists?: boolean;
}

export interface WorktreeCleanupReport {
  removed_worktrees: string[];
  deleted_branches: string[];
  skipped_dirty: string[];
  skipped_unmerged: string[];
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
  /** Branch for the isolated worktree; omit to auto-generate. */
  branch?: string;
  /** When `branch` is set: create it (true) or check out an existing one (false). */
  create_branch?: boolean;
  /** Start point for a newly created branch; defaults to HEAD. */
  base_ref?: string;
  /** Selected agent-option toggles (e.g. permission-skipping flags), keyed by option key. */
  options?: Record<string, boolean>;
}

/**
 * A fork inherits its origin's daemon, workspace and working directory, so none
 * of those are here. The only two choices a fork has are which agent to hand the
 * work to, and where it runs relative to the origin's branch.
 */
export interface ForkSessionBody {
  /** The agent to fork into — the origin's own, or a different one. */
  agent_plugin_id: string;
  /**
   * Continue on the origin's branch, sharing its worktree, instead of branching
   * off it into a new one. Safe once the origin has stopped; while it is still
   * running this puts two agents in one directory.
   */
  same_branch?: boolean;
  options?: Record<string, boolean>;
}

export interface BranchList {
  branches: string[];
  head: string | null;
}

/** Where a client-side VS Code should connect to reach a session's workspace. */
export interface VscodeTarget {
  path: string;
  ssh_user: string | null;
  hostname: string;
}

import { Target } from "./connectionStore";
import i18n from "./i18n";

/** Base URL with any trailing slash stripped ("" targets the local origin). */
function baseOf(baseUrl: string): string {
  return baseUrl.replace(/\/$/, "");
}

// fetch rejects with an opaque TypeError when the host is unreachable
// (connection refused, DNS, offline) — name the likely cause instead.
function unreachableError(baseUrl: string): Error {
  return new Error(
    baseUrl ? i18n.t("api.unreachableAt", { baseUrl }) : i18n.t("api.unreachable"),
  );
}

/**
 * Message for a non-OK response: the JSON body's `error` when present
 * (`fromBody: true`), else `status statusText`.
 */
async function errorMessage(res: Response): Promise<{ msg: string; fromBody: boolean }> {
  let msg = `${res.status} ${res.statusText}`;
  let fromBody = false;
  try {
    const body = await res.json();
    if (body?.error) {
      msg = body.error;
      fromBody = true;
    }
  } catch {
    /* ignore */
  }
  return { msg, fromBody };
}

async function req<T>(t: Target, path: string, init?: RequestInit): Promise<T> {
  const headers: Record<string, string> = {
    "content-type": "application/json",
    ...((init?.headers as Record<string, string>) ?? {}),
  };
  if (t.token) headers["Authorization"] = `Bearer ${t.token}`;
  if (t.relayKey) headers["X-ASM-Relay-Key"] = t.relayKey;

  let res: Response;
  try {
    res = await fetch(baseOf(t.baseUrl) + path, { ...init, headers });
  } catch {
    throw unreachableError(t.baseUrl);
  }
  if (!res.ok) {
    const { msg: bodyMsg, fromBody } = await errorMessage(res);
    let msg = bodyMsg;
    if (res.status === 401) {
      msg = i18n.t("api.unauthorized", { message: msg });
    } else if (!fromBody && (res.status === 502 || res.status === 504)) {
      // A bare gateway error means a proxy sits in front of a dead daemon.
      msg = i18n.t("api.gatewayUnreachable", { message: msg });
    }
    // Expose the HTTP status so callers can branch on it (e.g. 409 → confirm
    // and retry a guarded, destructive action with force).
    throw Object.assign(new Error(msg), { status: res.status });
  }
  return res.json() as Promise<T>;
}

/**
 * POST raw binary (e.g. an attached file or a pasted image Blob) and parse a
 * JSON reply. Mirrors `req`'s auth handling but sends the Blob as-is — fetch
 * derives the multipart boundary-free `Content-Type` from the Blob, so we don't
 * force JSON.
 */
async function postBlob<T>(t: Target, path: string, blob: Blob): Promise<T> {
  const headers: Record<string, string> = {
    "content-type": blob.type || "application/octet-stream",
  };
  if (t.token) headers["Authorization"] = `Bearer ${t.token}`;
  if (t.relayKey) headers["X-ASM-Relay-Key"] = t.relayKey;

  let res: Response;
  try {
    res = await fetch(baseOf(t.baseUrl) + path, { method: "POST", headers, body: blob });
  } catch {
    throw unreachableError(t.baseUrl);
  }
  if (!res.ok) {
    const { msg } = await errorMessage(res);
    throw Object.assign(new Error(msg), { status: res.status });
  }
  return res.json() as Promise<T>;
}

/**
 * GET raw bytes as a Blob (e.g. an image file for the diff preview). Mirrors
 * `req`'s auth handling but returns the body untouched, so the caller can wrap
 * it in an object URL — `<img>` can't carry an Authorization header itself.
 */
async function getBlob(t: Target, path: string): Promise<Blob> {
  const headers: Record<string, string> = {};
  if (t.token) headers["Authorization"] = `Bearer ${t.token}`;
  if (t.relayKey) headers["X-ASM-Relay-Key"] = t.relayKey;

  let res: Response;
  try {
    res = await fetch(baseOf(t.baseUrl) + path, { headers });
  } catch {
    throw unreachableError(t.baseUrl);
  }
  if (!res.ok) {
    const { msg } = await errorMessage(res);
    throw Object.assign(new Error(msg), { status: res.status });
  }
  return res.blob();
}

/** A file the daemon offers for download, with the name it chose for it. */
export interface Download {
  blob: Blob;
  /** From `Content-Disposition`; null if the daemon didn't name the file. */
  filename: string | null;
}

/**
 * GET a file to save. Like `getBlob`, but keeps the daemon's filename: the
 * transcript endpoint picks the extension (`.md` for a rendered conversation,
 * `.log` when it can only serve the raw stream), so the client can't derive it.
 */
async function getDownload(t: Target, path: string): Promise<Download> {
  const headers: Record<string, string> = {};
  if (t.token) headers["Authorization"] = `Bearer ${t.token}`;
  if (t.relayKey) headers["X-ASM-Relay-Key"] = t.relayKey;

  let res: Response;
  try {
    res = await fetch(baseOf(t.baseUrl) + path, { headers });
  } catch {
    throw unreachableError(t.baseUrl);
  }
  if (!res.ok) {
    const { msg } = await errorMessage(res);
    throw Object.assign(new Error(msg), { status: res.status });
  }
  return { blob: await res.blob(), filename: contentDispositionName(res) };
}

/** The `filename="…"` of a `Content-Disposition` header, if it carries one. */
export function contentDispositionName(res: Response): string | null {
  const cd = res.headers.get("Content-Disposition");
  const m = cd?.match(/filename="([^"]+)"/);
  return m ? m[1] : null;
}

/** Where a stored attachment landed on the daemon host. */
export interface StoredAttachment {
  path: string;
  relative_path: string;
  filename: string;
}

/**
 * Largest attachment the daemon will store (`MAX_PASTE_BYTES`). Mirrored here so
 * the client can reject an oversize file instantly with a clear message instead
 * of uploading it just to collect a 413. The daemon still enforces it.
 */
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;

/**
 * Enroll a device against a specific daemon; returns its device token. When the
 * daemon is reached through a relay, pass `relayKey` so the relay authorizes the
 * (public, at the daemon layer) enroll request.
 */
export async function enrollDevice(
  baseUrl: string,
  enrollmentToken: string,
  deviceName: string,
  relayKey?: string | null,
): Promise<{ server_id: string; device_id: string; device_token: string }> {
  const b = baseOf(baseUrl);
  const headers: Record<string, string> = { "content-type": "application/json" };
  if (relayKey) headers["X-ASM-Relay-Key"] = relayKey;
  let res: Response;
  try {
    res = await fetch(b + "/api/auth/enroll", {
      method: "POST",
      headers,
      body: JSON.stringify({ enrollment_token: enrollmentToken, device_name: deviceName }),
    });
  } catch {
    throw unreachableError(baseUrl);
  }
  if (!res.ok) throw new Error((await errorMessage(res)).msg);
  return res.json();
}

/** Probe a daemon's /health (used to validate a connection). */
export async function probeHealth(
  baseUrl: string,
  token: string | null,
  relayKey?: string | null,
): Promise<Health> {
  const b = baseOf(baseUrl);
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  if (relayKey) headers["X-ASM-Relay-Key"] = relayKey;
  let res: Response;
  try {
    res = await fetch(b + "/health", { headers });
  } catch {
    throw unreachableError(baseUrl);
  }
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

export const api = {
  health: (t: Target) => req<Health>(t, "/health"),
  listPlugins: (t: Target) =>
    req<{ plugins: PluginInfo[] }>(t, "/api/plugins").then((r) => r.plugins),
  listSessions: (t: Target) =>
    req<{ sessions: Session[] }>(t, "/api/sessions").then((r) => r.sessions),
  getSummary: (t: Target, id: string) =>
    req<{ summary: SessionSummary }>(t, `/api/sessions/${id}/summary`).then((r) => r.summary),
  /**
   * Full conversation as a raw terminal transcript (ANSI included), as a Blob so
   * the browser can save it. No delta — always the complete recorded stream.
   */
  sessionTranscript: (t: Target, id: string) =>
    getDownload(t, `/api/sessions/${id}/transcript`),
  sessionUsage: (t: Target, id: string) =>
    req<{ usage: SessionUsage }>(t, `/api/sessions/${id}/usage`).then((r) => r.usage),
  createSession: (t: Target, body: CreateSessionBody) =>
    req<{ session: Session }>(t, "/api/sessions", {
      method: "POST",
      body: JSON.stringify(body),
    }).then((r) => r.session),
  /**
   * Fork a session into a new one carrying its context. Slow by nature — the
   * daemon may run an agent headlessly to write the handoff brief — so callers
   * should show progress rather than assume this returns promptly.
   */
  forkSession: (t: Target, id: string, body: ForkSessionBody) =>
    req<{ session: Session }>(t, `/api/sessions/${id}/fork`, {
      method: "POST",
      body: JSON.stringify(body),
    }).then((r) => r.session),
  stopSession: (t: Target, id: string) =>
    req<{ session: Session }>(t, `/api/sessions/${id}/stop`, { method: "POST" }).then(
      (r) => r.session,
    ),
  archiveSession: (t: Target, id: string, force = false) =>
    req<{ session: Session }>(
      t,
      `/api/sessions/${id}/archive${force ? "?force=true" : ""}`,
      { method: "POST" },
    ).then((r) => r.session),
  ackAttention: (t: Target, id: string) =>
    req<{ session: Session }>(t, `/api/sessions/${id}/ack`, { method: "POST" }).then(
      (r) => r.session,
    ),
  resizeSession: (t: Target, id: string, rows: number, cols: number) =>
    req<{ ok: boolean }>(t, `/api/sessions/${id}/resize`, {
      method: "POST",
      body: JSON.stringify({ rows, cols }),
    }),
  scmStatus: (t: Target, id: string) =>
    req<{ status: ScmStatus }>(t, `/api/sessions/${id}/scm/status`).then((r) => r.status),
  scmDiff: (t: Target, id: string, path: string, untracked: boolean, commit?: string) =>
    req<{ path: string; diff: string }>(
      t,
      `/api/sessions/${id}/scm/diff?path=${encodeURIComponent(path)}&untracked=${untracked}` +
        (commit ? `&commit=${encodeURIComponent(commit)}` : ""),
    ).then((r) => r.diff),
  /**
   * One side of a changed file's image preview, as a Blob. `side` picks the
   * new content (`after`) or the prior version (`before`); a side with no
   * content (a new file's before, a deleted file's after) rejects with 404.
   */
  scmFile: (t: Target, id: string, path: string, side: "before" | "after", commit?: string) =>
    getBlob(
      t,
      `/api/sessions/${id}/scm/file?path=${encodeURIComponent(path)}&side=${side}` +
        (commit ? `&commit=${encodeURIComponent(commit)}` : ""),
    ),
  scmLog: (t: Target, id: string, limit = 30) =>
    req<{ commits: Commit[] }>(t, `/api/sessions/${id}/scm/log?limit=${limit}`).then(
      (r) => r.commits,
    ),
  scmCommit: (t: Target, id: string, hash: string) =>
    req<{ commit: CommitDetail }>(
      t,
      `/api/sessions/${id}/scm/commit?hash=${encodeURIComponent(hash)}`,
    ).then((r) => r.commit),
  scmBranches: (t: Target, id: string) =>
    req<BranchList>(t, `/api/sessions/${id}/scm/branches`),
  scmFetch: (t: Target, id: string) =>
    req<{ output: string }>(t, `/api/sessions/${id}/scm/fetch`, { method: "POST" }).then(
      (r) => r.output,
    ),
  scmPull: (t: Target, id: string) =>
    req<{ output: string }>(t, `/api/sessions/${id}/scm/pull`, { method: "POST" }).then(
      (r) => r.output,
    ),
  scmPush: (t: Target, id: string) =>
    req<{ output: string }>(t, `/api/sessions/${id}/scm/push`, { method: "POST" }).then(
      (r) => r.output,
    ),
  scmDetachBranch: (t: Target, id: string) =>
    req<{ branch: string; attached: false }>(t, `/api/sessions/${id}/scm/detach-branch`, {
      method: "POST",
    }).then((r) => r.branch),
  scmAttachBranch: (t: Target, id: string) =>
    req<{ branch: string; attached: true }>(t, `/api/sessions/${id}/scm/attach-branch`, {
      method: "POST",
    }).then((r) => r.branch),
  scmRebase: (t: Target, id: string, onto: string) =>
    req<{ output: string }>(t, `/api/sessions/${id}/scm/rebase`, {
      method: "POST",
      body: JSON.stringify({ onto }),
    }).then((r) => r.output),
  scmMerge: (t: Target, id: string, target: string) =>
    req<{ output: string }>(t, `/api/sessions/${id}/scm/merge`, {
      method: "POST",
      body: JSON.stringify({ target }),
    }).then((r) => r.output),
  listWorkspaces: (t: Target) =>
    req<{ workspaces: Workspace[] }>(t, "/api/workspaces").then((r) => r.workspaces),
  addWorkspace: (t: Target, name: string, root_path: string) =>
    req<{ workspace: Workspace }>(t, "/api/workspaces", {
      method: "POST",
      body: JSON.stringify({ name, root_path }),
    }).then((r) => r.workspace),
  removeWorkspace: (t: Target, id: string) =>
    req<{ ok: boolean }>(t, `/api/workspaces/${id}`, { method: "DELETE" }),
  cleanupWorktrees: (t: Target, id: string, force: boolean) =>
    req<{ report: WorktreeCleanupReport }>(
      t,
      `/api/workspaces/${id}/cleanup-worktrees?force=${force}`,
      { method: "POST" },
    ).then((r) => r.report),
  initWorkspaceGit: (t: Target, id: string) =>
    req<{ workspace: Workspace }>(t, `/api/workspaces/${id}/init-git`, {
      method: "POST",
    }).then((r) => r.workspace),
  workspaceBranches: (t: Target, id: string) =>
    req<BranchList>(t, `/api/workspaces/${id}/branches`),
  sessionWorkspace: (t: Target, id: string) =>
    req<{ instance: WorkspaceInstance | null }>(t, `/api/sessions/${id}/workspace`).then(
      (r) => r.instance,
    ),
  cleanupInstance: (t: Target, id: string, force: boolean) =>
    req<{ ok: boolean }>(t, `/api/sessions/${id}/cleanup?force=${force}`, {
      method: "POST",
    }),
  vscodeTarget: (t: Target, id: string) =>
    req<VscodeTarget>(t, `/api/sessions/${id}/vscode-target`),
  fsList: (t: Target, path: string, showHidden: boolean) =>
    req<FsListing>(
      t,
      `/api/fs/list?path=${encodeURIComponent(path)}&show_hidden=${showHidden}`,
    ),
  enrollmentToken: (t: Target) =>
    req<{ enrollment_token: string }>(t, "/api/auth/enrollment-token").then(
      (r) => r.enrollment_token,
    ),
  /**
   * Upload a pasted/dropped/picked attachment of any type; the daemon stores it
   * under the session's working directory and returns the path to inject into
   * the terminal. `name` is advisory — it only shapes the stored filename (the
   * daemon sanitises it and picks the directory itself). A clipboard image is a
   * bare Blob with no name, so it's optional.
   */
  uploadAttachment: (t: Target, id: string, blob: Blob, name?: string) =>
    postBlob<StoredAttachment>(
      t,
      `/api/sessions/${id}/paste` + (name ? `?name=${encodeURIComponent(name)}` : ""),
      blob,
    ),
};

export function streamUrl(t: Target, id: string): string {
  let base: string;
  if (t.baseUrl) {
    const u = new URL(t.baseUrl);
    const proto = u.protocol === "https:" ? "wss" : "ws";
    // Preserve any path prefix (e.g. `/n/<node_id>` for a relayed daemon) —
    // dropping it would bypass the relay route. Strip a trailing slash so we
    // don't double it before `/api`.
    const prefix = u.pathname.replace(/\/$/, "");
    base = `${proto}://${u.host}${prefix}`;
  } else {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    base = `${proto}://${location.host}`;
  }
  const params: string[] = [];
  if (t.token) params.push(`access_token=${encodeURIComponent(t.token)}`);
  // Browsers cannot set WS headers, so the relay key rides as a query param.
  if (t.relayKey) params.push(`relay_key=${encodeURIComponent(t.relayKey)}`);
  const query = params.length ? `?${params.join("&")}` : "";
  return `${base}/api/sessions/${id}/stream${query}`;
}

/** One node the relay knows about (mirrors asm-relay's NodeEntry). */
export interface RelayNode {
  node_id: string;
  label: string;
  kind: "leaf" | "gateway";
  via: string | null;
  online: boolean;
  last_seen: string;
}

/** Discover the nodes registered with a relay (requires the relay access key). */
export async function listRelayNodes(url: string, accessKey: string): Promise<RelayNode[]> {
  const b = url.replace(/\/$/, "");
  let res: Response;
  try {
    res = await fetch(b + "/nodes", { headers: { "X-ASM-Relay-Key": accessKey } });
  } catch {
    throw new Error(i18n.t("api.unreachableAt", { baseUrl: url }));
  }
  if (!res.ok) {
    if (res.status === 401) throw new Error(i18n.t("relay.errBadKey"));
    throw new Error(`${res.status} ${res.statusText}`);
  }
  const body = (await res.json()) as { nodes: RelayNode[] };
  return body.nodes ?? [];
}
