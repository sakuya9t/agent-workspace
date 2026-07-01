import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, Session, SessionStatus, AttentionState } from "../api";
import { useUiStore } from "../store";

const STATUS_COLOR: Record<SessionStatus, string> = {
  starting: "#e0af68",
  running: "#9ece6a",
  exited: "#565f89",
  failed: "#f7768e",
  stopped: "#565f89",
  archived: "#414868",
};

const ATTENTION_LABEL: Partial<Record<AttentionState, string>> = {
  activity: "new",
  likely_blocked: "blocked",
  approval_needed: "approve",
  failed: "failed",
};

const ATTENTION_COLOR: Partial<Record<AttentionState, string>> = {
  activity: "#7aa2f7",
  likely_blocked: "#e0af68",
  approval_needed: "#f7768e",
  failed: "#f7768e",
};

function isLive(s: SessionStatus): boolean {
  return s === "running" || s === "starting";
}

function relTime(ms: number): string {
  const d = Date.now() - ms;
  if (d < 5000) return "just now";
  if (d < 60000) return `${Math.floor(d / 1000)}s ago`;
  if (d < 3600000) return `${Math.floor(d / 60000)}m ago`;
  return `${Math.floor(d / 3600000)}h ago`;
}

export function SessionList() {
  const qc = useQueryClient();
  const activeId = useUiStore((s) => s.activeSessionId);
  const setActive = useUiStore((s) => s.setActive);
  const setShowNew = useUiStore((s) => s.setShowNewSession);

  const { data: sessions, error } = useQuery({
    queryKey: ["sessions"],
    queryFn: api.listSessions,
    refetchInterval: 1500,
  });

  const stop = useMutation({
    mutationFn: (id: string) => api.stopSession(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["sessions"] }),
  });
  const archive = useMutation({
    mutationFn: (id: string) => api.archiveSession(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["sessions"] }),
  });
  const ack = useMutation({
    mutationFn: (id: string) => api.ackAttention(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["sessions"] }),
  });

  const select = (s: Session) => {
    setActive(s.id);
    if (s.attention_state !== "none") ack.mutate(s.id);
  };

  return (
    <div className="panel sessions">
      <div className="panel-header">
        <span>Sessions</span>
        <button className="btn primary" onClick={() => setShowNew(true)}>
          + New
        </button>
      </div>
      <div className="panel-body">
        {error && <div className="error">Cannot reach daemon: {String(error)}</div>}
        {sessions?.length === 0 && (
          <div className="empty">No sessions yet. Create one to begin.</div>
        )}
        {sessions?.map((s) => (
          <div
            key={s.id}
            className={"session-row" + (s.id === activeId ? " active" : "")}
            onClick={() => select(s)}
          >
            <div className="session-main">
              <span
                className="status-dot"
                style={{ background: STATUS_COLOR[s.status] }}
                title={s.status}
              />
              <span className="session-agent">{s.agent_plugin_id}</span>
              {s.attention_state !== "none" && (
                <span
                  className="attn-badge"
                  style={{ background: ATTENTION_COLOR[s.attention_state] }}
                >
                  {ATTENTION_LABEL[s.attention_state]}
                </span>
              )}
            </div>
            <div className="session-sub">
              <span className="mono">{basename(s.working_directory)}</span>
              <span className="dim">{relTime(s.last_activity_at)}</span>
            </div>
            <div className="session-actions">
              {isLive(s.status) ? (
                <button
                  className="btn tiny"
                  onClick={(e) => {
                    e.stopPropagation();
                    stop.mutate(s.id);
                  }}
                >
                  stop
                </button>
              ) : (
                s.status !== "archived" && (
                  <button
                    className="btn tiny"
                    onClick={(e) => {
                      e.stopPropagation();
                      archive.mutate(s.id);
                    }}
                  >
                    archive
                  </button>
                )
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function basename(p: string): string {
  const parts = p.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : p;
}
