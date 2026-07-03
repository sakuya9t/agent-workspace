import { useQuery } from "@tanstack/react-query";
import { api, SessionUsage } from "../api";
import { Target } from "../connectionStore";

interface Props {
  target: Target;
  sessionId: string;
  agent: string;
  onClose: () => void;
}

/**
 * Per-session usage popup — token/context and rate-limit info read from the
 * agent's own on-disk transcript (the same data behind `/status` / `/usage`).
 * Best-effort: the daemon matches the newest agent transcript for the session.
 */
export function UsageModal({ target, sessionId, agent, onClose }: Props) {
  const { data, error, isLoading } = useQuery({
    queryKey: ["usage", target.baseUrl, sessionId],
    queryFn: () => api.sessionUsage(target, sessionId),
    retry: false,
    refetchInterval: 5000,
  });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal usage-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">
          <span>
            Session usage <span className="dim">· {agent}</span>
          </span>
          <button className="btn tiny" onClick={onClose}>
            close
          </button>
        </div>

        <div className="usage-view">
          {isLoading && <div className="dim">Reading usage…</div>}
          {error && <div className="error">{String(error)}</div>}
          {data && !data.available && (
            <div className="dim">{data.note ?? "No usage data available for this session."}</div>
          )}
          {data && data.available && <UsageBody u={data} />}
        </div>
      </div>
    </div>
  );
}

function UsageBody({ u }: { u: SessionUsage }) {
  const ctxPct =
    u.context_tokens != null && u.context_window
      ? Math.min(100, (u.context_tokens / u.context_window) * 100)
      : null;

  return (
    <>
      {u.model && (
        <div className="usage-model mono">{u.model}</div>
      )}

      {u.context_tokens != null && (
        <div className="usage-block">
          <div className="usage-block-head">
            <span>Context window</span>
            <span className="mono">
              {fmt(u.context_tokens)}
              {u.context_window ? ` / ${fmt(u.context_window)}` : ""}
              {ctxPct != null ? ` · ${ctxPct.toFixed(0)}%` : ""}
            </span>
          </div>
          {ctxPct != null && (
            <div className="usage-bar">
              <div className={"usage-bar-fill " + heat(ctxPct)} style={{ width: `${ctxPct}%` }} />
            </div>
          )}
        </div>
      )}

      <div className="usage-grid">
        <Stat label="Input" value={u.input_tokens} />
        <Stat label="Cached input" value={u.cached_input_tokens} />
        <Stat label="Output" value={u.output_tokens} />
        {u.reasoning_tokens != null && <Stat label="Reasoning" value={u.reasoning_tokens} />}
        {u.total_tokens != null && <Stat label="Total" value={u.total_tokens} strong />}
      </div>

      {u.rate_limits.length > 0 && (
        <div className="usage-block">
          <div className="usage-block-head">
            <span>Rate limits</span>
          </div>
          {u.rate_limits.map((r, i) => (
            <div className="usage-rl" key={i}>
              <div className="usage-block-head sub">
                <span>{r.label}</span>
                <span className="mono">
                  {r.used_percent.toFixed(1)}%{r.resets_at ? ` · resets ${fmtReset(r.resets_at)}` : ""}
                </span>
              </div>
              <div className="usage-bar">
                <div
                  className={"usage-bar-fill " + heat(r.used_percent)}
                  style={{ width: `${Math.min(100, r.used_percent)}%` }}
                />
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="usage-foot dim">
        {u.updated_at && <div>Reading: {u.updated_at}</div>}
        {u.source && <div className="mono usage-src">{u.source}</div>}
        {u.note && <div>{u.note}</div>}
      </div>
    </>
  );
}

function Stat({ label, value, strong }: { label: string; value: number | null; strong?: boolean }) {
  return (
    <div className={"usage-stat" + (strong ? " strong" : "")}>
      <div className="usage-stat-label">{label}</div>
      <div className="usage-stat-value mono">{value != null ? fmt(value) : "—"}</div>
    </div>
  );
}

function fmt(n: number): string {
  return n.toLocaleString();
}

function heat(pct: number): string {
  if (pct >= 90) return "hot";
  if (pct >= 70) return "warm";
  return "cool";
}

function fmtReset(unixSecs: number): string {
  const ms = unixSecs * 1000;
  const diff = ms - Date.now();
  if (diff <= 0) return "now";
  const mins = Math.round(diff / 60000);
  if (mins < 60) return `in ${mins}m`;
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  if (hours < 24) return rem ? `in ${hours}h ${rem}m` : `in ${hours}h`;
  const days = Math.floor(hours / 24);
  return `in ${days}d ${hours % 24}h`;
}
