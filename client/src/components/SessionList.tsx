import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, Session, SessionStatus, AttentionState, Workspace } from "../api";
import { daemonLabel, Target, targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import { DaemonState, useDaemonStates } from "../useDaemons";
import { isLive } from "../status";
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

// Blocked (waiting on the user) is orange; error/failed (something went wrong)
// is red — the badge color alone should say which kind of attention it is.
const ATTENTION_COLOR: Partial<Record<AttentionState, string>> = {
  activity: "#7aa2f7",
  idle: "#565f89",
  likely_blocked: "#ff9e64",
  approval_needed: "#ff9e64",
  error: "#f7768e",
  failed: "#f7768e",
};

type MutArgs = { target: Target; id: string };

export function SessionList() {
  const { t } = useTranslation();
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
  // Archiving removes the session from history and deletes its branch. The click
  // is already confirmed; this second prompt is the escalation — the daemon
  // guards uncommitted/unmerged work with a 409, so name what would be lost and
  // retry with force only if the user still wants it gone.
  const archive = useMutation({
    mutationFn: async ({ target, id }: MutArgs) => {
      try {
        return await api.archiveSession(target, id);
      } catch (e) {
        if ((e as { status?: number }).status === 409) {
          if (confirm(t("sessionList.confirmArchiveForce", { message: (e as Error).message }))) {
            return api.archiveSession(target, id, true);
          }
          return; // declined — leave it in history
        }
        throw e;
      }
    },
    onSuccess: refresh,
    onError: (e) => alert(String(e)),
  });
  // Save the full conversation to a file the browser downloads. The daemon
  // renders the agent's own transcript to Markdown and names the file (auth
  // headers ride the fetch), so we wrap the Blob in an object URL and click a
  // synthetic link. No delta — every save is the complete conversation. Not
  // offered for archived sessions (discarded).
  const save = useMutation({
    mutationFn: async ({ target, s }: { target: Target; s: Session }) => {
      const { blob, filename } = await api.sessionTranscript(target, s.id);
      triggerDownload(blob, filename ?? transcriptFilename(s));
    },
    onError: (e) =>
      alert(t("sessionList.saveError", { message: e instanceof Error ? e.message : String(e) })),
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
      if (!confirm(t("sessionList.confirmTakeOver"))) {
        return;
      }
    }
    setActive({ daemonId, sessionId: s.id });
    if (s.attention_state !== "none") ack.mutate({ target, id: s.id });
  };

  // History aggregates finished sessions across all daemons: ended but not yet
  // archived. Archiving is the deliberate "throw this away" step — it drops the
  // session from history and deletes its branch, so archived sessions are hidden
  // here. Workspace names are resolved per daemon; a session whose workspace was
  // since removed (or an ad-hoc session) falls back to its working directory.
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
      if (!isLive(s.status) && s.status !== "archived")
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
    const name = sessionLabel(s, ctx?.workspaceName);
    const title = sessionTitle(s, ctx?.workspaceName);
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
          <span className="session-title" title={title}>
            {title}
          </span>
          <SessionInfo s={s} />
          {s.risky && (
            <span className="risk-badge" title={t("sessionList.riskTitle")}>
              {t("sessionList.riskBadge")}
            </span>
          )}
          {ctx?.daemonLabel && <span className="daemon-tag">{ctx.daemonLabel}</span>}
          {s.attached && !selected && isLive(s.status) && (
            <span
              className="attn-badge"
              style={{ background: "#565f89" }}
              title={t("sessionList.inUseTitle")}
            >
              {t("sessionList.inUse")}
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
          <span>
            {s.agent_plugin_id}
            {ctx?.workspaceName ? ` · ${ctx.workspaceName}` : ""}
          </span>
          <span className="dim">{relTime(s.last_activity_at)}</span>
        </div>
        <div className="session-actions">
          {isLive(s.status) ? (
            <button
              className="icon-btn"
              title={t("sessionList.stopTitle")}
              aria-label={t("sessionList.stopTitle")}
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(t("sessionList.confirmStop", { name }))) {
                  stop.mutate({ target, id: s.id });
                }
              }}
            >
              <span className="action-icon action-icon-stop" aria-hidden="true" />
            </button>
          ) : (
            <span className="ended-status" title={statusLabel(s.status)}>
              {endedLabel(s.status)}
              {s.exit_code !== null ? ` · ${s.exit_code}` : ""}
            </span>
          )}
          {/* Save works for any non-archived session (live or ended); archived
              sessions have been discarded, so there's nothing to save. */}
          {s.status !== "archived" && (
            <button
              className="icon-btn"
              title={t("sessionList.saveTitle")}
              aria-label={t("sessionList.saveTitle")}
              disabled={save.isPending && save.variables?.s.id === s.id}
              onClick={(e) => {
                e.stopPropagation();
                save.mutate({ target, s });
              }}
            >
              <span className="action-icon action-icon-save-transcript" aria-hidden="true" />
            </button>
          )}
          {!isLive(s.status) && s.status !== "archived" && (
            <button
              className="icon-btn"
              title={t("sessionList.archiveTitle")}
              aria-label={t("sessionList.archiveTitle")}
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(t("sessionList.confirmArchive", { name }))) {
                  archive.mutate({ target, id: s.id });
                }
              }}
            >
              <span className="action-icon action-icon-archive" aria-hidden="true" />
            </button>
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
            title={
              missing ? t("sessionList.missingTitle", { path: w.root_path }) : w.root_path
            }
            style={missing ? { color: "#f7768e" } : undefined}
          >
            {w.name}
          </span>
          {missing ? (
            <span
              className="tree-sub"
              style={{ color: "#f7768e" }}
              title={t("sessionList.missingTitle", { path: w.root_path })}
            >
              {t("common.missing")}
            </span>
          ) : (
            <span className="tree-sub">
              {w.is_git ? t("common.git") : t("common.plain")}
            </span>
          )}
          <div className="tree-actions">
            {sessions.length > 0 && <span className="tree-badge">{sessions.length}</span>}
            <button
              className="tree-add"
              title={t("sessionList.newSessionTitle")}
              onClick={(e) => {
                e.stopPropagation();
                openNewSession(daemonId, w.id);
              }}
            >
              +
            </button>
            <button
              className="tree-add"
              title={t("sessionList.removeWsTitle")}
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(t("sessionList.confirmRemoveWorkspace", { name: w.name }))) {
                  removeWs.mutate({ target, id: w.id });
                }
              }}
            >
              ×
            </button>
          </div>
        </div>
        {open && (
          <div className="tree-leaves">
            {sessions.length ? (
              sessions.map((s) => row(daemonId, target, s, { workspaceName: w.name }))
            ) : (
              <div className="tree-empty">{t("sessionList.noActiveSessions")}</div>
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
    const adhocKey = daemon.id + ":adhoc";

    return (
      <div key={daemon.id} className={"tree-branch" + (connected ? "" : " disconnected")}>
        <div className="tree-node lvl0" onClick={() => toggle(daemon.id)}>
          <span className="chevron">{open ? "▾" : "▸"}</span>
          <span className="tree-icon">⬢</span>
          <span className="tree-label" title={daemonLabel(daemon)}>
            {daemonLabel(daemon)}
          </span>
          <span className="tree-sub">
            {!connected
              ? t("sessionList.disconnected")
              : bundle
                ? `${bundle.health.hostname} · ${bundle.health.platform}`
                : unreachable
                  ? t("sessionList.unreachable")
                  : t("sessionList.connecting")}
          </span>
          <div className="tree-actions">
            {connected && bundle && <span className="tree-badge">{active.length}</span>}
            {connected && (
              <button
                className="tree-add"
                title={t("sessionList.newWorkspaceTitle")}
                onClick={(e) => {
                  e.stopPropagation();
                  openNewWorkspace(daemon.id);
                }}
              >
                +
              </button>
            )}
            <button
              className="tree-add conn-toggle"
              title={
                connected
                  ? t("sessionList.disconnectTitle")
                  : t("sessionList.connectTitle")
              }
              aria-label={
                connected
                  ? t("sessionList.disconnectTitle")
                  : t("sessionList.connectTitle")
              }
              onClick={(e) => {
                e.stopPropagation();
                updateDaemon(daemon.id, { connected: !connected });
              }}
            >
              <span
                className={
                  "action-icon " +
                  (connected ? "action-icon-disconnect-daemon" : "action-icon-connect-daemon")
                }
                aria-hidden="true"
              />
            </button>
          </div>
        </div>

        {open && !connected && (
          <div className="tree-empty">{t("sessionList.disconnectedNotPolling")}</div>
        )}

        {open && unreachable && (
          <div className="tree-empty error-line">
            {daemon.baseUrl || t("sessionList.local")} —{" "}
            {(st.error as Error)?.message ?? t("sessionList.unreachable")}
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
                <div className="tree-node lvl2" onClick={() => toggle(adhocKey)}>
                  <span className="chevron">{isOpen(adhocKey) ? "▾" : "▸"}</span>
                  <span className="tree-icon">▫</span>
                  <span className="tree-label">{t("sessionList.adhoc")}</span>
                  <div className="tree-actions">
                    <span className="tree-badge">{adhoc.length}</span>
                  </div>
                </div>
                {isOpen(adhocKey) && (
                  <div className="tree-leaves">
                    {adhoc.map((s) => row(daemon.id, target, s))}
                  </div>
                )}
              </div>
            )}
            {bundle.workspaces.length === 0 && adhoc.length === 0 && (
              <div className="tree-empty">{t("sessionList.noActiveSessionsDot")}</div>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="panel sessions">
      <div className="panel-header">
        <span>{t("sessionList.sessionsHeader")}</span>
        <div className="header-actions">
          <button
            className="btn tiny"
            onClick={() => setShowConnection(true)}
            title={t("sessionList.manageTitle")}
          >
            {t("sessionList.daemonsBtn")}
          </button>
          <button className="btn primary" onClick={() => openNewSession(null, null)}>
            {t("sessionList.newBtn")}
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
            <span>{t("sessionList.historyHeader")}</span>
            <span className="history-count">{history.length}</span>
          </div>
          {historyOpen && (
            <div className="history-list">
              {history.map(({ daemon, target, s, workspaceName }) =>
                row(daemon.id, target, s, { daemonLabel: daemonLabel(daemon), workspaceName }),
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * The session's info button: hover (pointer devices) or tap pops a small
 * fixed-position panel with the identifiers the row no longer shows — branch,
 * session uuid, directory. Fixed positioning because the list is a scroll
 * container that would clip an absolutely-positioned child; any outside click
 * or scroll dismisses it.
 */
function SessionInfo({ s }: { s: Session }) {
  const { t } = useTranslation();
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  // Hover previews (mouseleave closes); a click pins the panel open so its
  // contents can be selected and copied. Outside click/scroll always closes.
  const [pinned, setPinned] = useState(false);

  useEffect(() => {
    if (!pos) return;
    const close = () => {
      setPos(null);
      setPinned(false);
    };
    document.addEventListener("click", close);
    document.addEventListener("scroll", close, true);
    return () => {
      document.removeEventListener("click", close);
      document.removeEventListener("scroll", close, true);
    };
  }, [pos]);

  const openAt = (el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setPos({
      top: r.bottom + 6,
      left: Math.max(8, Math.min(r.left, window.innerWidth - 328)),
    });
  };

  return (
    <>
      <button
        className="info-btn"
        title={t("sessionList.infoTitle")}
        aria-label={t("sessionList.infoTitle")}
        onClick={(e) => {
          // Not a toggle: on touch the tap fires mouseenter first, and a toggle
          // would immediately close what the hover just opened.
          e.stopPropagation();
          openAt(e.currentTarget);
          setPinned(true);
        }}
        onMouseEnter={(e) => openAt(e.currentTarget)}
        onMouseLeave={() => {
          if (!pinned) setPos(null);
        }}
      />
      {pos && (
        <div className="info-pop" style={pos} onClick={(e) => e.stopPropagation()}>
          <div className="info-row">
            <span>{t("sessionList.infoBranch")}</span>
            <span className="mono">{s.branch ?? "—"}</span>
          </div>
          <div className="info-row">
            <span>{t("sessionList.infoId")}</span>
            <span className="mono">{s.id}</span>
          </div>
          <div className="info-row">
            <span>{t("sessionList.infoPath")}</span>
            <span className="mono">{s.working_directory}</span>
          </div>
        </div>
      )}
    </>
  );
}

function basename(p: string): string {
  const parts = p.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : p;
}

/** The row's primary label: the agent's own title for the session, else the
 * workspace/directory naming the row showed before titles existed. */
function sessionTitle(s: Session, workspaceName?: string): string {
  return s.title ?? workspaceName ?? basename(s.working_directory);
}

/**
 * How a session is named in a confirm dialog: "claude · Fix the flaky test".
 * Rows carry no visible id, so the dialog has to echo back what the row
 * showed — otherwise a mis-click is confirmed just as readily as the intended
 * one.
 */
function sessionLabel(s: Session, workspaceName?: string): string {
  return `${s.agent_plugin_id} · ${sessionTitle(s, workspaceName)}`;
}

/**
 * Fallback download filename, for a daemon that sent no `Content-Disposition`
 * (an older one). `.md` matches what current daemons serve.
 */
function transcriptFilename(s: Session): string {
  const agent = s.agent_plugin_id.replace(/[^A-Za-z0-9._-]/g, "_");
  return `session-${agent}-${s.id}.md`;
}

/** Save a Blob to the user's machine via a synthetic download link. */
function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  // Revoke on the next tick — some browsers abort the download if the object
  // URL is released synchronously during the click.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
