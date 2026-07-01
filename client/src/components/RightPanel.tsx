import { useQuery } from "@tanstack/react-query";
import { api, Session } from "../api";

interface Props {
  session: Session | undefined;
}

/**
 * Right control-center panel. Shows session metadata and the structural
 * summary once a session ends. The Git changed-files / diff panel lands here
 * in the next iteration (Phase 6-7).
 */
export function RightPanel({ session }: Props) {
  const terminal =
    session &&
    ["exited", "failed", "stopped", "archived"].includes(session.status);

  const { data: summary } = useQuery({
    queryKey: ["summary", session?.id],
    queryFn: () => api.getSummary(session!.id),
    enabled: !!session && !!terminal,
    retry: false,
  });

  if (!session) {
    return (
      <div className="panel right">
        <div className="panel-header">Details</div>
        <div className="panel-body">
          <div className="empty">Select a session.</div>
        </div>
      </div>
    );
  }

  return (
    <div className="panel right">
      <div className="panel-header">Details</div>
      <div className="panel-body details">
        <Field label="Agent" value={session.agent_plugin_id} />
        <Field label="Status" value={session.status} />
        <Field label="Command" value={[session.command, ...session.args].join(" ")} mono />
        <Field label="Directory" value={session.working_directory} mono />
        <Field label="Size" value={`${session.cols}×${session.rows}`} />
        <Field label="Events" value={String(session.last_event_seq)} />
        {session.exit_code !== null && (
          <Field label="Exit code" value={String(session.exit_code)} />
        )}
        {session.attention_state !== "none" && (
          <Field
            label="Attention"
            value={`${session.attention_state}${
              session.attention_reason ? ` — ${session.attention_reason}` : ""
            }`}
          />
        )}

        {summary && (
          <>
            <div className="section-title">Session summary</div>
            <Field label="Exit" value={summary.exit_status} />
            <Field label="Duration" value={`${Math.round(summary.duration_ms / 100) / 10}s`} />
            <Field
              label="Event range"
              value={`${summary.terminal_event_start}–${summary.terminal_event_end}`}
            />
          </>
        )}

        <div className="section-title">Source control</div>
        <div className="dim small">
          Git changed-files &amp; diffs arrive in the next iteration.
        </div>
      </div>
    </div>
  );
}

function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="field">
      <div className="field-label">{label}</div>
      <div className={"field-value" + (mono ? " mono" : "")}>{value}</div>
    </div>
  );
}
