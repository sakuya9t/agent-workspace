import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, ChangedFile, Commit, Session } from "../api";
import { Target } from "../connectionStore";
import { buildVscodeLaunch, launchVscode, VscodeLaunch } from "../vscode";
import { relTime } from "../i18n/time";
import { attentionLabel, instanceStatusLabel, isolationLabel, statusLabel } from "../i18n/labels";
import { DiffModal } from "./DiffModal";
import { CommitModal } from "./CommitModal";

interface Props {
  target: Target | undefined;
  session: Session | undefined;
}

type VscodeState =
  | { phase: "idle" }
  | { phase: "launching" }
  | { phase: "opened"; launch: VscodeLaunch }
  | { phase: "not-installed"; launch: VscodeLaunch }
  | { phase: "error"; message: string };

const STATUS_COLOR: Record<string, string> = {
  A: "#9ece6a",
  M: "#e0af68",
  D: "#f7768e",
  R: "#7aa2f7",
  C: "#7aa2f7",
  U: "#f7768e",
  "?": "#7b86a1",
};

/**
 * Right control-center panel: session metadata, the structural summary once a
 * session ends, and the Git changed-files list with click-to-diff.
 */
export function RightPanel({ target, session }: Props) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [diffTarget, setDiffTarget] = useState<ChangedFile | null>(null);
  const [commitTarget, setCommitTarget] = useState<string | null>(null);

  const terminal =
    session &&
    ["exited", "failed", "stopped", "archived"].includes(session.status);
  const base = target?.baseUrl ?? "";

  const { data: instance } = useQuery({
    queryKey: ["instance", base, session?.id],
    queryFn: () => api.sessionWorkspace(target!, session!.id),
    enabled: !!session && !!target,
    retry: false,
  });

  const cleanup = useMutation({
    mutationFn: (force: boolean) => api.cleanupInstance(target!, session!.id, force),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["instance", base, session?.id] }),
  });

  // "Continue in VS Code" launches the editor on the *client's* machine via a
  // vscode:// deep link (Remote-SSH for remote daemons). If nothing handles
  // the link, fall back to offering the VS Code download — the Zoom-link UX.
  const [vscode, setVscode] = useState<VscodeState>({ phase: "idle" });
  const [copied, setCopied] = useState(false);
  // Reset on daemon switch too: the same session reached through another
  // profile launches differently (local vs Remote-SSH).
  useEffect(() => setVscode({ phase: "idle" }), [session?.id, base]);
  useEffect(() => setCopied(false), [vscode.phase, session?.id]);

  const copyCli = async (text: string) => {
    try {
      // clipboard API needs a secure context; plain-http LAN profiles don't
      // have one, so fall back to the selection-based path.
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    setCopied(true);
  };

  const continueInVscode = async () => {
    setVscode({ phase: "launching" });
    try {
      const info = await api.vscodeTarget(target!, session!.id);
      const launch = buildVscodeLaunch(target!, info);
      const opened = await launchVscode(launch.uri);
      setVscode({ phase: opened ? "opened" : "not-installed", launch });
    } catch (e) {
      setVscode({ phase: "error", message: String(e) });
    }
  };

  const { data: summary } = useQuery({
    queryKey: ["summary", base, session?.id],
    queryFn: () => api.getSummary(target!, session!.id),
    enabled: !!session && !!target && !!terminal,
    retry: false,
  });

  const { data: scm } = useQuery({
    queryKey: ["scm", base, session?.id],
    queryFn: () => api.scmStatus(target!, session!.id),
    enabled: !!session && !!target,
    refetchInterval: 2500,
    retry: false,
  });

  const { data: commits } = useQuery({
    queryKey: ["scmlog", base, session?.id],
    queryFn: () => api.scmLog(target!, session!.id, 30),
    enabled: !!session && !!target && !!scm?.is_repo,
    refetchInterval: 5000,
    retry: false,
  });

  if (!session || !target) {
    return (
      <div className="panel right">
        <div className="panel-header">{t("rightPanel.header")}</div>
        <div className="panel-body">
          <div className="empty">{t("rightPanel.empty")}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="panel right">
      <div className="panel-header">{t("rightPanel.header")}</div>
      <div className="panel-body details">
        <button
          className="btn vscode-btn"
          disabled={vscode.phase === "launching"}
          onClick={continueInVscode}
          title={t("rightPanel.vscode.title")}
        >
          {vscode.phase === "launching"
            ? t("rightPanel.vscode.opening")
            : t("rightPanel.vscode.button")}
        </button>
        {vscode.phase === "opened" && (
          <div className="dim small">
            {vscode.launch.kind === "remote-ssh"
              ? t("rightPanel.vscode.openingRemote", { dest: vscode.launch.sshDest })
              : t("rightPanel.vscode.openingLocal")}
          </div>
        )}
        {vscode.phase === "not-installed" && (
          <div className="vscode-fallback">
            <div>{t("rightPanel.vscode.didntOpen")}</div>
            <a
              className="btn tiny"
              href="https://code.visualstudio.com/download"
              target="_blank"
              rel="noreferrer"
            >
              {t("rightPanel.vscode.download")}
            </a>
            {vscode.launch.kind === "remote-ssh" && (
              <div className="dim small">
                {t("rightPanel.vscode.remoteSshHint", { dest: vscode.launch.sshDest })}
              </div>
            )}
            {vscode.launch.cliCommand && (
              <>
                <div className="dim small">{t("rightPanel.vscode.cliHint")}</div>
                <div className="cli-row">
                  <code>{vscode.launch.cliCommand}</code>
                  <button
                    className="btn tiny"
                    onClick={() => copyCli(vscode.launch.cliCommand!)}
                  >
                    {copied ? t("common.copied") : t("common.copy")}
                  </button>
                </div>
              </>
            )}
          </div>
        )}
        {vscode.phase === "error" && <div className="error">{vscode.message}</div>}

        {session.risky && (
          <div className="risk-banner" title={t("rightPanel.riskBannerTitle")}>
            {t("rightPanel.riskBanner")}
          </div>
        )}

        <Field label={t("rightPanel.fieldAgent")} value={session.agent_plugin_id} />
        <Field label={t("rightPanel.fieldStatus")} value={statusLabel(session.status)} />
        <Field
          label={t("rightPanel.fieldCommand")}
          value={[session.command, ...session.args].join(" ")}
          mono
        />
        <Field label={t("rightPanel.fieldDirectory")} value={session.working_directory} mono />
        {instance && (
          <>
            <Field
              label={t("rightPanel.fieldWorkspaceInstance")}
              value={`${isolationLabel(instance.isolation)}${
                instance.branch ? ` · ${instance.branch}` : ""
              }${instance.status === "released" ? ` · ${instanceStatusLabel(instance.status)}` : ""}`}
            />
            {instance.isolation === "worktree" && instance.status !== "released" && terminal && (
              <div className="instance-actions">
                <button
                  className="btn tiny"
                  disabled={cleanup.isPending}
                  onClick={() => cleanup.mutate(false)}
                >
                  {t("rightPanel.cleanupWorktree")}
                </button>
                <button
                  className="btn tiny"
                  disabled={cleanup.isPending}
                  onClick={() => cleanup.mutate(true)}
                >
                  {t("rightPanel.force")}
                </button>
              </div>
            )}
            {cleanup.error && <div className="error">{String(cleanup.error)}</div>}
          </>
        )}
        <Field label={t("rightPanel.fieldSize")} value={`${session.cols}×${session.rows}`} />
        {session.exit_code !== null && (
          <Field label={t("rightPanel.fieldExitCode")} value={String(session.exit_code)} />
        )}
        {session.attention_state !== "none" && (
          <Field
            label={t("rightPanel.fieldAttention")}
            value={`${attentionLabel(session.attention_state)}${
              session.attention_reason ? ` — ${session.attention_reason}` : ""
            }`}
          />
        )}

        {summary && (
          <>
            <div className="section-title">{t("rightPanel.summaryHeader")}</div>
            <Field label={t("rightPanel.fieldExit")} value={summary.exit_status} />
            <Field
              label={t("rightPanel.fieldDuration")}
              value={t("rightPanel.seconds", {
                count: Math.round(summary.duration_ms / 100) / 10,
              })}
            />
            <Field
              label={t("rightPanel.fieldEventRange")}
              value={`${summary.terminal_event_start}–${summary.terminal_event_end}`}
            />
          </>
        )}

        <div className="section-title">
          {t("rightPanel.scmHeader")}
          {scm?.is_repo && (
            <span className="branch-pill mono">
              {scm.detached ? t("rightPanel.detached") : scm.branch}
              {scm.head ? ` · ${scm.head}` : ""}
            </span>
          )}
        </div>

        {!scm?.is_repo && (
          <div className="dim small">{t("rightPanel.notRepo", { cmd: "git init" })}</div>
        )}

        {scm?.is_repo && scm.changed_files.length === 0 && (
          <div className="dim small">{t("rightPanel.treeClean")}</div>
        )}

        {scm?.is_repo &&
          scm.changed_files.map((f) => (
            <div
              key={f.path}
              className="changed-file"
              onClick={() => setDiffTarget(f)}
              title={t("rightPanel.viewDiff")}
            >
              <span
                className="change-badge"
                style={{ color: STATUS_COLOR[f.status] ?? "#c7d0e0" }}
              >
                {f.status}
              </span>
              <span className="mono change-path">{shortPath(f.path)}</span>
              {f.staged && <span className="staged-dot" title={t("rightPanel.staged")} />}
            </div>
          ))}

        {scm?.is_repo && (
          <>
            <div className="section-title">{t("rightPanel.historyHeader")}</div>
            {commits && commits.length > 0 ? (
              <CommitGraph commits={commits} head={scm.head} onSelect={setCommitTarget} />
            ) : (
              <div className="dim small">
                {commits ? t("rightPanel.noCommits") : t("rightPanel.loadingHistory")}
              </div>
            )}
          </>
        )}
      </div>

      {diffTarget && (
        <DiffModal
          target={target}
          sessionId={session.id}
          path={diffTarget.path}
          untracked={diffTarget.untracked}
          onClose={() => setDiffTarget(null)}
        />
      )}

      {commitTarget && (
        <CommitModal
          target={target}
          sessionId={session.id}
          hash={commitTarget}
          onClose={() => setCommitTarget(null)}
        />
      )}
    </div>
  );
}

/**
 * Simplified single-lane commit graph for the MVP (per the architecture doc's
 * "closest history model"): a vertical rail with one dot per commit, newest at
 * the top. Merge commits (>1 parent) get a hollow dot; the HEAD commit is
 * highlighted.
 */
function CommitGraph({
  commits,
  head,
  onSelect,
}: {
  commits: Commit[];
  head: string | null;
  onSelect: (hash: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="commit-graph">
      {commits.map((c, i) => {
        const merge = c.parents.length > 1;
        const isHead = !!head && c.short === head;
        return (
          <div
            className="commit-row"
            key={c.hash}
            title={t("rightPanel.viewCommit")}
            onClick={() => onSelect(c.hash)}
          >
            <div
              className={
                "commit-rail" +
                (i === 0 ? " first" : "") +
                (i === commits.length - 1 ? " last" : "")
              }
            >
              <span className={"commit-dot" + (merge ? " merge" : "") + (isHead ? " head" : "")} />
            </div>
            <div className="commit-body">
              <div className="commit-subject">
                {c.subject || t("rightPanel.noMessage")}
                {isHead && <span className="head-pill">HEAD</span>}
              </div>
              <div className="commit-meta">
                <span className="mono">{c.short}</span>
                <span className="dim"> · {c.author} · {relTime(c.timestamp * 1000)}</span>
              </div>
            </div>
          </div>
        );
      })}
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

function shortPath(p: string): string {
  const parts = p.split("/");
  if (parts.length <= 3) return p;
  return ".../" + parts.slice(-2).join("/");
}
