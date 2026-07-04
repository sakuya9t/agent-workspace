import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, Session, SessionStatus, AttentionState, Workspace } from "../api";
import { Target, targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import { DaemonState, useDaemonStates } from "../useDaemons";
import { relTime } from "../i18n/time";
import { attentionLabel, endedLabel, statusLabel } from "../i18n/labels";

const STATUS_COLOR: Record<SessionStatus, string> = {
  starting: "#e0af68",
  running: "#9ece6a",
  exited: "#565f89",
  failed: "#f7768e",
  stopped: "#565f89",
  archived: "#414868",
  indeterminate: "#ff9e64",
};

const ATTENTION_COLOR: Partial<Record<AttentionState, string>> = {
  activity: "#7aa2f7",
  idle: "#565f89",
  likely_blocked: "#e0af68",
  approval_needed: "#f7768e",
  failed: "#f7768e",
};

function isLive(s: SessionStatus): boolean {
  return s === "running" || s === "starting";
}

type MutArgs = { target: Target; id: string };

export function SessionList() {
  const qc = useQueryClient();
  const active = useUiStore((s) => s.activeSession);
  const setActive = useUiStore((s) => s.setActive);
  const openNewSession = useUiStore((s) => s.openNewSession);
  const openNewWorkspace = useUiStore((s) => s.openNewWorkspace);
  const setShowConnection = useUiStore((s) => s.setShowConnection);
  const updateDaemon = useConnStore((s) => s.updateDaemon);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const states = useDaemonStates();

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  const isOpen = (id: string) => !collapsed.has(id);
  const refresh = () => qc.invalidateQueries({ queryKey: ["daemon"] });

  const stop = useMutation({
    mutationFn: ({ target, id }: MutArgs) => api.stopSession(target, id),
    onSuccess: refresh,
  });
  const archive = useMutation({
    mutationFn: ({ target, id }: MutArgs) => api.archiveSession(target, id),
    onSuccess: refresh,
  });
  const ack = useMutation({
    mutationFn: ({ target, id }: MutArgs) => api.ackAttention(target, id),
    onSuccess: refresh,
  });
  const removeWs = useMutation({
    mutationFn: ({ target, id }: MutArgs) => api.removeWorkspace(target, id),
    onSuccess: refresh,
    onError: (e) => alert(String(e)),
  });

  const select = (daemonId: string, target: Target, s: Session) => {
    const isMine = active?.daemonId === daemonId && active?.sessionId === s.id;
    // Single-attacher: opening a session another client holds takes it over,
    // disconnecting them — so confirm first.
    if (s.attached && !isMine && isLive(s.status)) {
      if (
        !confirm(
          "This session is open on another client.\n\nTake over? The other client will be disconnected from it.",
        )
      ) {
        return;
      }
    }
    setActive({ daemonId, sessionId: s.id });
    if (s.attention_state !== "none") ack.mutate({ target, id: s.id });
  };

  // History aggregates ended sessions across all daemons. Workspace names are
  // resolved per daemon; a session whose workspace was since removed (or an
  // ad-hoc session) falls back to its working directory in the row.
  const history: {
    daemon: DaemonState["daemon"];
    target: Target;
    s: Session;
    workspaceName?: string;
  }[] = [];
  for (const st of states) {
    if (!st.data) continue;
    const target = targetOf(st.daemon);
    const wsNames = new Map(st.data.workspaces.map((w) => [w.id, w.name]));
    for (const s of st.data.sessions) {
      if (!isLive(s.status))
        history.push({
          daemon: st.daemon,
          target,
          s,
          workspaceName: s.workspace_id ? wsNames.get(s.workspace_id) : undefined,
        });
    }
  }
  history.sort((a, b) => b.s.last_activity_at - a.s.last_activity_at);

  const row = (
    daemonId: string,
    target: Target,
    s: Session,
    ctx?: { daemonLabel?: string; workspaceName?: string },
  ) => {
    const selected = active?.daemonId === daemonId && active?.sessionId === s.id;
    return (
      <div
        key={daemonId + ":" + s.id}
        className={"session-row" + (selected ? " active" : "")}
        onClick={() => select(daemonId, target, s)}
      >
        <div className="session-main">
          <span
            className="status-dot"
            style={{ background: STATUS_COLOR[s.status] }}
            title={statusLabel(s.status)}
          />
          <span className="session-agent">{s.agent_plugin_id}</span>
          {s.risky && (
            <span
              className="risk-badge"
              title="Launched with guardrails disabled (skip-permissions / bypass-sandbox)"
            >
              ⚠ unsafe
            </span>
          )}
          {ctx?.daemonLabel && <span className="daemon-tag">{ctx.daemonLabel}</span>}
          {s.attached && !selected && isLive(s.status) && (
            <span
              className="attn-badge"
              style={{ background: "#565f89" }}
              title="Open on another client — click to take over"
            >
              in use
            </span>
          )}
          {s.attention_state !== "none" && (
            <span
              className="attn-badge"
              style={{ background: ATTENTION_COLOR[s.attention_state] }}
            >
              {attentionLabel(s.attention_state)}
            </span>
          )}
        </div>
        <div className="session-sub">
          <span className="mono" title={s.working_directory}>
            {ctx?.workspaceName ?? basename(s.working_directory)}
          </span>
          <span className="dim">{relTime(s.last_activity_at)}</span>
        </div>
        <div className="session-actions">
          {isLive(s.status) ? (
            <button
              className="btn tiny"
              onClick={(e) => {
                e.stopPropagation();
                stop.mutate({ target, id: s.id });
              }}
            >
              stop
            </button>
          ) : (
            <>
              <span className="ended-status" title={statusLabel(s.status)}>
                {endedLabel(s.status)}
                {s.exit_code !== null ? ` · ${s.exit_code}` : ""}
              </span>
              {s.status !== "archived" && (
                <button
                  className="btn tiny"
                  onClick={(e) => {
                    e.stopPropagation();
                    archive.mutate({ target, id: s.id });
                  }}
                >
                  archive
                </button>
              )}
            </>
          )}
        </div>
      </div>
    );
  };

  const workspaceNode = (
    daemonId: string,
    target: Target,
    w: Workspace,
    sessions: Session[],
  ) => {
    const key = daemonId + ":ws:" + w.id;
    const open = isOpen(key);
    const missing = w.root_exists === false;
    return (
      <div key={key} className="tree-branch">
        <div className="tree-node lvl2" onClick={() => toggle(key)}>
          <span className="chevron">{open ? "▾" : "▸"}</span>
          <span className="tree-icon">{w.is_git ? "◆" : "▪"}</span>
          <span
            className="tree-label"
            title={missing ? `${w.root_path} — no longer exists on the host` : w.root_path}
            style={missing ? { color: "#f7768e" } : undefined}
          >
            {w.name}
          </span>
          {missing ? (
            <span
              className="tree-sub"
              style={{ color: "#f7768e" }}
              title={`${w.root_path} — no longer exists on the host`}
            >
              missing
            </span>
          ) : (
            <span className="tree-sub">{w.is_git ? "git" : "plain"}</span>
          )}
          {sessions.length > 0 && <span className="tree-badge">{sessions.length}</span>}
          <button
            className="tree-add"
            title="New session in this workspace"
            onClick={(e) => {
              e.stopPropagation();
              openNewSession(daemonId, w.id);
            }}
          >
            +
          </button>
          <button
            className="tree-add"
            title="Remove (unregister) this workspace"
            onClick={(e) => {
              e.stopPropagation();
              if (
                confirm(
                  `Remove workspace "${w.name}"?\n\nThis only unregisters it from this daemon — sessions and files on disk are left intact.`,
                )
              ) {
                removeWs.mutate({ target, id: w.id });
              }
            }}
          >
            ×
          </button>
        </div>
        {open && (
          <div className="tree-leaves">
            {sessions.length ? (
              sessions.map((s) => row(daemonId, target, s))
            ) : (
              <div className="tree-empty">no active sessions</div>
            )}
          </div>
        )}
      </div>
    );
  };

  const daemonNode = (st: DaemonState) => {
    const { daemon } = st;
    const connected = daemon.connected;
    const target = targetOf(daemon);
    const open = isOpen(daemon.id);
    // Ignore any stale cache while disconnected — a disconnected host shows no
    // sessions, just a "connect" affordance.
    const bundle = connected ? st.data : undefined;
    // Only treat a daemon as unreachable when we have NO cached data. A single
    // dropped poll keeps the last data, so the tree stays stable (no flicker).
    const unreachable = connected && Boolean(st.error) && !bundle;
    const active = bundle?.sessions.filter((s) => isLive(s.status)) ?? [];
    const wsIds = new Set((bundle?.workspaces ?? []).map((w) => w.id));
    const adhoc = active.filter((s) => !s.workspace_id || !wsIds.has(s.workspace_id));

    return (
      <div key={daemon.id} className={"tree-branch" + (connected ? "" : " disconnected")}>
        <div className="tree-node lvl0" onClick={() => toggle(daemon.id)}>
          <span className="chevron">{open ? "▾" : "▸"}</span>
          <span className="tree-icon">⬢</span>
          <span className="tree-label">{daemon.label}</span>
          <span className="tree-sub">
            {!connected
              ? "disconnected"
              : bundle
                ? `${bundle.health.hostname} · ${bundle.health.platform}`
                : unreachable
                  ? "unreachable"
                  : "connecting…"}
          </span>
          {connected && bundle && <span className="tree-badge">{active.length}</span>}
          {connected && (
            <button
              className="tree-add"
              title="New workspace on this host"
              onClick={(e) => {
                e.stopPropagation();
                openNewWorkspace(daemon.id);
              }}
            >
              +
            </button>
          )}
          <button
            className="btn tiny conn-toggle"
            title={
              connected
                ? "Disconnect — keep the host listed but stop polling it"
                : "Connect — resume polling with the same token"
            }
            onClick={(e) => {
              e.stopPropagation();
              updateDaemon(daemon.id, { connected: !connected });
            }}
          >
            {connected ? "disconnect" : "connect"}
          </button>
        </div>

        {open && !connected && (
          <div className="tree-empty">disconnected — not polling</div>
        )}

        {open && unreachable && (
          <div className="tree-empty error-line">
            {daemon.baseUrl || "local"} —{" "}
            {(st.error as Error)?.message ?? "unreachable"}
          </div>
        )}

        {open && connected && bundle && (
          <div className="tree-children">
            {bundle.workspaces.map((w) =>
              workspaceNode(
                daemon.id,
                target,
                w,
                active.filter((s) => s.workspace_id === w.id),
              ),
            )}
            {adhoc.length > 0 && (
              <div className="tree-branch">
                <div
                  className="tree-node lvl2"
                  onClick={() => toggle(daemon.id + ":adhoc")}
                >
                  <span className="chevron">
                    {isOpen(daemon.id + ":adhoc") ? "▾" : "▸"}
                  </span>
                  <span className="tree-icon">▫</span>
                  <span className="tree-label">Ad-hoc directories</span>
                  <span className="tree-badge">{adhoc.length}</span>
                </div>
                {isOpen(daemon.id + ":adhoc") && (
                  <div className="tree-leaves">
                    {adhoc.map((s) => row(daemon.id, target, s))}
                  </div>
                )}
              </div>
            )}
            {bundle.workspaces.length === 0 && adhoc.length === 0 && (
              <div className="tree-empty">No active sessions.</div>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="panel sessions">
      <div className="panel-header">
        <span>Sessions</span>
        <div className="header-actions">
          <button
            className="btn tiny"
            onClick={() => setShowConnection(true)}
            title="Manage daemons"
          >
            daemons
          </button>
          <button className="btn primary" onClick={() => openNewSession(null, null)}>
            + New
          </button>
        </div>
      </div>

      <div className="panel-body">
        <div className="tree">{states.map(daemonNode)}</div>
      </div>

      {history.length > 0 && (
        <div className={"history-section" + (historyOpen ? " open" : "")}>
          <div className="history-header" onClick={() => setHistoryOpen((v) => !v)}>
            <span className="chevron">{historyOpen ? "▾" : "▸"}</span>
            <span>History</span>
            <span className="history-count">{history.length}</span>
          </div>
          {historyOpen && (
            <div className="history-list">
              {history.map(({ daemon, target, s, workspaceName }) =>
                row(daemon.id, target, s, { daemonLabel: daemon.label, workspaceName }),
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function basename(p: string): string {
  const parts = p.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : p;
}
