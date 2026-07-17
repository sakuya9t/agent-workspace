import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, BaseCommit, ChangedFile, Commit, Session } from "../api";
import { Target } from "../connectionStore";
import { buildVscodeLaunch, launchVscode, vscodeReachable, VscodeLaunch } from "../vscode";
import { relTime } from "../i18n/time";
import { attentionLabel, instanceStatusLabel, isolationLabel, statusLabel } from "../i18n/labels";
import { isTerminal } from "../status";
import { useIsPhone } from "../useIsPhone";
import { shortPath } from "../paths";
import { copyText } from "../clipboard";
import { DiffModal } from "./DiffModal";
import { CommitModal } from "./CommitModal";

interface Props {
  target: Target | undefined;
  session: Session | undefined;
  /** Submit the same prompt a user would type into the live session TUI. */
  onCommitChanges?: () => void;
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
export function RightPanel({ target, session, onCommitChanges }: Props) {
  const { t } = useTranslation();
  // On a phone there's no local VS Code for the vscode:// deep link to reach, so
  // the whole "Continue in VS Code" affordance is hidden (mobile shell only).
  const isPhone = useIsPhone();
  const qc = useQueryClient();
  const [diffTarget, setDiffTarget] = useState<ChangedFile | null>(null);
  const [commitTarget, setCommitTarget] = useState<string | null>(null);
  const [rebaseOpen, setRebaseOpen] = useState(false);
  const [rebaseOnto, setRebaseOnto] = useState("");
  const [mergeOpen, setMergeOpen] = useState(false);
  const [mergeTarget, setMergeTarget] = useState("");

  const terminal = session && isTerminal(session.status);
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
    await copyText(text);
    setCopied(true);
  };

  // Relayed nodes have no direct route from the client, so no vscode:// deep
  // link can reach them (see vscode.ts). Disable the button rather than fire a
  // link that would SSH into the relay machine.
  const vscodeCanReach = !!target && vscodeReachable(target);

  const continueInVscode = async () => {
    if (!vscodeCanReach) return;
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

  // Branch list backing the rebase/merge target pickers; only fetched while a
  // picker is open so it doesn't poll needlessly.
  const { data: branchList, error: branchesError } = useQuery({
    queryKey: ["scmbranches", base, session?.id],
    queryFn: () => api.scmBranches(target!, session!.id),
    enabled: !!session && !!target && !!scm?.is_repo && (rebaseOpen || mergeOpen),
    retry: false,
  });

  // Refresh status + history after a source-control operation changes refs.
  const refreshScm = () => {
    qc.invalidateQueries({ queryKey: ["scm", base, session?.id] });
    qc.invalidateQueries({ queryKey: ["scmlog", base, session?.id] });
  };

  // Named `fetchRemotes`, not `fetch`, so it can't shadow the global.
  const fetchRemotes = useMutation({
    mutationFn: () => api.scmFetch(target!, session!.id),
    onSuccess: refreshScm,
  });

  const pull = useMutation({
    mutationFn: () => api.scmPull(target!, session!.id),
    onSuccess: refreshScm,
  });

  const push = useMutation({
    mutationFn: () => api.scmPush(target!, session!.id),
    onSuccess: refreshScm,
  });

  const rebase = useMutation({
    mutationFn: (onto: string) => api.scmRebase(target!, session!.id, onto),
    onSuccess: () => {
      setRebaseOpen(false);
      setRebaseOnto("");
      refreshScm();
    },
  });

  const merge = useMutation({
    mutationFn: (targetBranch: string) => api.scmMerge(target!, session!.id, targetBranch),
    onSuccess: () => {
      setMergeOpen(false);
      setMergeTarget("");
      refreshScm();
    },
  });

  // `attached` here means the managed worktree currently holds its recorded
  // branch. Detaching leaves the files and HEAD in place, but releases Git's
  // branch lock so the source checkout can temporarily deploy/verify it.
  const branchAttachment = useMutation({
    mutationFn: (attached: boolean) =>
      attached
        ? api.scmAttachBranch(target!, session!.id)
        : api.scmDetachBranch(target!, session!.id),
    onSuccess: refreshScm,
  });

  // Every source-control operation shares one result/error area, so starting any
  // one of them clears the others' stale output.
  const scmOps = [fetchRemotes, pull, push, rebase, merge, branchAttachment];
  const resetScmOps = () => scmOps.forEach((op) => op.reset());
  const startFetch = () => {
    resetScmOps();
    fetchRemotes.mutate();
  };
  const startPull = () => {
    resetScmOps();
    pull.mutate();
  };
  const startPush = () => {
    resetScmOps();
    push.mutate();
  };
  const startRebase = (onto: string) => {
    resetScmOps();
    rebase.mutate(onto);
  };
  const startMerge = (targetBranch: string) => {
    resetScmOps();
    merge.mutate(targetBranch);
  };
  const startBranchAttachment = (attached: boolean) => {
    resetScmOps();
    branchAttachment.mutate(attached);
  };
  const scmBusy = scmOps.some((op) => op.isPending);

  const recordedBranch = instance?.branch ?? null;
  const canToggleBranchAttachment =
    !!recordedBranch &&
    (instance?.isolation === "worktree" || instance?.isolation === "shared") &&
    (!!scm?.detached || scm?.branch === recordedBranch);
  const branchIsAttached = !!recordedBranch && scm?.branch === recordedBranch;

  // Don't carry an open picker or a previous session's SCM output onto the next
  // session (this panel is reused, not remounted, across selections).
  useEffect(() => {
    setRebaseOpen(false);
    setRebaseOnto("");
    setMergeOpen(false);
    setMergeTarget("");
    resetScmOps();
  }, [session?.id, base]);

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
        {!isPhone && (
          <>
            <button
              className="btn vscode-btn"
              disabled={vscode.phase === "launching" || !vscodeCanReach}
              onClick={continueInVscode}
              title={
                vscodeCanReach
                  ? t("rightPanel.vscode.title")
                  : t("rightPanel.vscode.relayUnavailableTitle")
              }
            >
              {vscode.phase === "launching"
                ? t("rightPanel.vscode.opening")
                : t("rightPanel.vscode.button")}
            </button>
            {!vscodeCanReach && (
              <div className="dim small">{t("rightPanel.vscode.relayUnavailable")}</div>
            )}
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
          </>
        )}

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

        <div className="section-title with-branch">
          <span>{t("rightPanel.scmHeader")}</span>
          {scm?.is_repo && (
            <span className="branch-controls">
              <span className="branch-pill mono">
                {scm.detached && recordedBranch
                  ? t("rightPanel.detachedFrom", { branch: recordedBranch })
                  : scm.detached
                    ? t("rightPanel.detached")
                    : scm.branch}
                {scm.head ? ` · ${scm.head}` : ""}
              </span>
              {canToggleBranchAttachment && (
                <button
                  className="icon-btn branch-attachment-btn"
                  disabled={scmBusy}
                  onClick={() => startBranchAttachment(!branchIsAttached)}
                  title={t(
                    branchIsAttached
                      ? "rightPanel.detachBranchTitle"
                      : "rightPanel.attachBranchTitle",
                    { branch: recordedBranch },
                  )}
                  aria-label={t(
                    branchIsAttached
                      ? "rightPanel.detachBranchTitle"
                      : "rightPanel.attachBranchTitle",
                    { branch: recordedBranch },
                  )}
                >
                  <span
                    className={`action-icon action-icon-git-${
                      branchIsAttached ? "detach" : "attach"
                    }`}
                    aria-hidden="true"
                  />
                </button>
              )}
            </span>
          )}
        </div>

        {scm?.is_repo && (scm.remotes.length > 0 || scm.base) && (
          <div className="scm-refs">
            {scm.remotes.map((r) => {
              const ref = `${r.remote}/${r.branch}`;
              return (
                <div className="scm-ref" key={r.remote}>
                  <span
                    className={"ref-tag" + (r.upstream ? " upstream" : "")}
                    title={t("rightPanel.remoteTitle", { ref })}
                  >
                    {r.remote}
                  </span>
                  <span className="mono ref-hash">{r.head}</span>
                  {/* The remote-side name, but only when it isn't the local one —
                      an upstream may track a differently-named branch. */}
                  <span className="ref-note dim">{r.branch === scm.branch ? "" : r.branch}</span>
                  <span className="ref-counts">
                    {r.ahead > 0 && (
                      <span
                        className="ref-count ahead"
                        title={t("rightPanel.remoteAhead", { count: r.ahead, ref })}
                      >
                        ↑{r.ahead}
                      </span>
                    )}
                    {r.behind > 0 && (
                      <span
                        className="ref-count behind"
                        title={t("rightPanel.remoteBehind", { count: r.behind, ref })}
                      >
                        ↓{r.behind}
                      </span>
                    )}
                  </span>
                </div>
              );
            })}

            {scm.base && (
              <div className="scm-ref">
                <span
                  className="ref-tag base"
                  title={t(
                    scm.base.kind === "spawned"
                      ? "rightPanel.baseSpawnedTitle"
                      : "rightPanel.baseRebasedTitle",
                    { branch: scm.branch },
                  )}
                >
                  {t("rightPanel.baseTag")}
                </span>
                <span className="mono ref-hash">{scm.base.short}</span>
                {/* Name the base by a ref that still points at it — "master" says
                    far more than a hash. Once they've all moved on, the commit's
                    own subject is what's left to identify it by. */}
                <span
                  className="ref-note dim"
                  title={scm.base.refs.join(", ") || scm.base.subject}
                >
                  {t(
                    scm.base.kind === "spawned"
                      ? "rightPanel.baseSpawnedFrom"
                      : "rightPanel.baseRebasedOnto",
                    { ref: scm.base.refs[0] || scm.base.subject },
                  )}
                </span>
                {scm.base.ahead > 0 && (
                  <span className="ref-counts">
                    <span
                      className="ref-count ahead"
                      title={t("rightPanel.baseAhead", { count: scm.base.ahead })}
                    >
                      +{scm.base.ahead}
                    </span>
                  </span>
                )}
              </div>
            )}
          </div>
        )}

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
            <div className="section-title with-actions">
              <span>{t("rightPanel.historyHeader")}</span>
              <span className="scm-actions">
                {!scm.detached && (
                  <button
                    className="icon-btn"
                    disabled={
                      !onCommitChanges || scmBusy || scm.changed_files.length === 0
                    }
                    onClick={onCommitChanges}
                    title={t("rightPanel.commitTitle")}
                    aria-label={t("rightPanel.commitTitle")}
                  >
                    <span className="action-icon action-icon-git-commit" aria-hidden="true" />
                  </button>
                )}
                {/* Fetch only reads the remotes, so unlike the rest it is just as
                    valid on a detached HEAD as on a branch. */}
                <button
                  className="icon-btn"
                  disabled={scmBusy}
                  onClick={startFetch}
                  title={t("rightPanel.fetchTitle")}
                  aria-label={t("rightPanel.fetchTitle")}
                >
                  <span className="action-icon action-icon-git-fetch" aria-hidden="true" />
                </button>
                {!scm.detached && (
                  <>
                    <button
                      className="icon-btn"
                      disabled={scmBusy}
                      onClick={startPull}
                      title={t("rightPanel.pullTitle")}
                      aria-label={t("rightPanel.pullTitle")}
                    >
                      <span className="action-icon action-icon-git-pull" aria-hidden="true" />
                    </button>
                    <button
                      className="icon-btn"
                      disabled={scmBusy}
                      onClick={startPush}
                      title={t("rightPanel.pushTitle")}
                      aria-label={t("rightPanel.pushTitle")}
                    >
                      <span className="action-icon action-icon-git-push" aria-hidden="true" />
                    </button>
                    <button
                      className={"icon-btn" + (rebaseOpen ? " active" : "")}
                      disabled={scmBusy}
                      onClick={() => {
                        setMergeOpen(false);
                        setRebaseOpen((o) => !o);
                      }}
                      title={t("rightPanel.rebaseTitle")}
                      aria-label={t("rightPanel.rebaseTitle")}
                    >
                      <span className="action-icon action-icon-git-rebase" aria-hidden="true" />
                    </button>
                    <button
                      className={"icon-btn" + (mergeOpen ? " active" : "")}
                      disabled={scmBusy}
                      onClick={() => {
                        setRebaseOpen(false);
                        setMergeOpen((o) => !o);
                      }}
                      title={t("rightPanel.mergeTitle")}
                      aria-label={t("rightPanel.mergeTitle")}
                    >
                      <span className="action-icon action-icon-git-merge" aria-hidden="true" />
                    </button>
                  </>
                )}
              </span>
            </div>

            {rebaseOpen && !scm.detached && (
              <div className="rebase-picker">
                <div className="rebase-picker-label">
                  {t("rightPanel.rebaseOnto", { branch: scm.branch })}
                </div>
                {(() => {
                  const candidates = branchList
                    ? branchList.branches.filter((b) => b !== branchList.head)
                    : [];
                  if (branchesError) {
                    return <div className="error">{String(branchesError)}</div>;
                  }
                  if (!branchList) {
                    return <div className="dim small">{t("rightPanel.loadingBranches")}</div>;
                  }
                  if (candidates.length === 0) {
                    return <div className="dim small">{t("rightPanel.noOtherBranches")}</div>;
                  }
                  return (
                    <div className="rebase-picker-row">
                      <select
                        className="rebase-select mono"
                        value={rebaseOnto}
                        disabled={scmBusy}
                        onChange={(e) => setRebaseOnto(e.target.value)}
                      >
                        <option value="" disabled>
                          {t("rightPanel.rebaseSelectPlaceholder")}
                        </option>
                        {candidates.map((b) => (
                          <option key={b} value={b}>
                            {b}
                          </option>
                        ))}
                      </select>
                      <button
                        className="btn tiny"
                        disabled={!rebaseOnto || scmBusy}
                        onClick={() => startRebase(rebaseOnto)}
                      >
                        {rebase.isPending
                          ? t("rightPanel.scmRunning")
                          : t("rightPanel.rebaseConfirm")}
                      </button>
                    </div>
                  );
                })()}
              </div>
            )}

            {mergeOpen && !scm.detached && (
              <div className="rebase-picker">
                <div className="rebase-picker-label">
                  {t("rightPanel.mergeInto", { branch: scm.branch })}
                </div>
                {(() => {
                  const candidates = branchList
                    ? branchList.branches.filter((b) => b !== branchList.head)
                    : [];
                  if (branchesError) {
                    return <div className="error">{String(branchesError)}</div>;
                  }
                  if (!branchList) {
                    return <div className="dim small">{t("rightPanel.loadingBranches")}</div>;
                  }
                  if (candidates.length === 0) {
                    return <div className="dim small">{t("rightPanel.noMergeBranches")}</div>;
                  }
                  return (
                    <div className="rebase-picker-row">
                      <select
                        className="rebase-select mono"
                        value={mergeTarget}
                        disabled={scmBusy}
                        onChange={(e) => setMergeTarget(e.target.value)}
                      >
                        <option value="" disabled>
                          {t("rightPanel.mergeSelectPlaceholder")}
                        </option>
                        {candidates.map((b) => (
                          <option key={b} value={b}>
                            {b}
                          </option>
                        ))}
                      </select>
                      <button
                        className="btn tiny"
                        disabled={!mergeTarget || scmBusy}
                        onClick={() => startMerge(mergeTarget)}
                      >
                        {merge.isPending
                          ? t("rightPanel.scmRunning")
                          : t("rightPanel.mergeConfirm")}
                      </button>
                    </div>
                  );
                })()}
              </div>
            )}

            {scmBusy && (
              <div className="dim small">{t("rightPanel.scmRunning")}</div>
            )}
            {fetchRemotes.error && (
              <ScmOpNotice
                status="error"
                title={t("rightPanel.fetchFailed")}
                summary={scmErrorSummary(fetchRemotes.error)}
                details={scmErrorDetails(fetchRemotes.error)}
                onDismiss={fetchRemotes.reset}
              />
            )}
            {pull.error && (
              <ScmOpNotice
                status="error"
                title={t("rightPanel.pullFailed")}
                summary={scmErrorSummary(pull.error)}
                details={scmErrorDetails(pull.error)}
                onDismiss={pull.reset}
              />
            )}
            {push.error && (
              <ScmOpNotice
                status="error"
                title={t("rightPanel.pushFailed")}
                summary={scmErrorSummary(push.error)}
                details={scmErrorDetails(push.error)}
                onDismiss={push.reset}
              />
            )}
            {rebase.error && (
              <ScmOpNotice
                status="error"
                title={t("rightPanel.rebaseFailed")}
                summary={scmErrorSummary(rebase.error)}
                details={scmErrorDetails(rebase.error)}
                onDismiss={rebase.reset}
              />
            )}
            {merge.error && (
              <ScmOpNotice
                status="error"
                title={t("rightPanel.mergeFailed")}
                summary={
                  (merge.error as { status?: number }).status === 409
                    ? t("rightPanel.mergeConflictSummary")
                    : scmErrorSummary(merge.error)
                }
                details={scmErrorDetails(merge.error)}
                onDismiss={merge.reset}
              />
            )}
            {branchAttachment.error && (
              <ScmOpNotice
                status="error"
                title={t(
                  branchAttachment.variables
                    ? "rightPanel.attachBranchFailed"
                    : "rightPanel.detachBranchFailed",
                )}
                summary={scmErrorSummary(branchAttachment.error)}
                details={scmErrorDetails(branchAttachment.error)}
                onDismiss={branchAttachment.reset}
              />
            )}
            {/* A fetch that found nothing new succeeds with *empty* output, so the
                truthiness check the other ops use would swallow the result. */}
            {fetchRemotes.data !== undefined && (
              <ScmOpNotice
                status="success"
                title={t("rightPanel.fetchComplete")}
                summary={
                  fetchRemotes.data.trim()
                    ? t("rightPanel.fetchSuccess")
                    : t("rightPanel.fetchUpToDate")
                }
                details={fetchRemotes.data}
                onDismiss={fetchRemotes.reset}
              />
            )}
            {pull.data && (
              <ScmOpNotice
                status="success"
                title={t("rightPanel.pullComplete")}
                summary={
                  pull.data.toLowerCase().includes("already up to date")
                    ? t("rightPanel.pullUpToDate", {
                        branch: scm.branch ?? t("rightPanel.currentBranch"),
                      })
                    : t("rightPanel.pullSuccess", {
                        branch: scm.branch ?? t("rightPanel.currentBranch"),
                      })
                }
                details={pull.data}
                onDismiss={pull.reset}
              />
            )}
            {push.data && (
              <ScmOpNotice
                status="success"
                title={t("rightPanel.pushComplete")}
                summary={
                  push.data.toLowerCase().includes("up-to-date") ||
                  push.data.toLowerCase().includes("up to date")
                    ? t("rightPanel.pushUpToDate", {
                        branch: scm.branch ?? t("rightPanel.currentBranch"),
                      })
                    : t("rightPanel.pushSuccess", {
                        branch: scm.branch ?? t("rightPanel.currentBranch"),
                      })
                }
                details={push.data}
                onDismiss={push.reset}
              />
            )}
            {rebase.data && (
              <ScmOpNotice
                status="success"
                title={t("rightPanel.rebaseComplete")}
                summary={
                  rebase.data.toLowerCase().includes("up to date")
                    ? t("rightPanel.rebaseUpToDate", {
                        branch: scm.branch ?? t("rightPanel.currentBranch"),
                        target: rebase.variables,
                      })
                    : t("rightPanel.rebaseSuccess", {
                        branch: scm.branch ?? t("rightPanel.currentBranch"),
                        target: rebase.variables,
                      })
                }
                details={rebase.data}
                onDismiss={rebase.reset}
              />
            )}
            {merge.data && (
              <ScmOpNotice
                status="success"
                title={t("rightPanel.mergeComplete")}
                summary={t("rightPanel.mergeSuccess", {
                  branch: scm.branch ?? t("rightPanel.currentBranch"),
                  target: merge.variables,
                })}
                details={merge.data}
                onDismiss={merge.reset}
              />
            )}
            {branchAttachment.data && (
              <ScmOpNotice
                status="success"
                title={t(
                  branchAttachment.variables
                    ? "rightPanel.attachBranchComplete"
                    : "rightPanel.detachBranchComplete",
                )}
                summary={t(
                  branchAttachment.variables
                    ? "rightPanel.attachBranchSuccess"
                    : "rightPanel.detachBranchSuccess",
                  { branch: branchAttachment.data },
                )}
                details=""
                onDismiss={branchAttachment.reset}
              />
            )}

            {commits && commits.length > 0 ? (
              <CommitGraph
                commits={commits}
                head={scm.head}
                base={scm.base}
                onSelect={setCommitTarget}
              />
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
 * highlighted and the branch's base commit is tagged.
 */
function CommitGraph({
  commits,
  head,
  base,
  onSelect,
}: {
  commits: Commit[];
  head: string | null;
  base: BaseCommit | null;
  onSelect: (hash: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="commit-graph">
      {commits.map((c, i) => {
        const merge = c.parents.length > 1;
        const isHead = !!head && c.short === head;
        const isBase = !!base && c.hash === base.hash;
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
                {isBase && <span className="base-pill">{t("rightPanel.baseTag")}</span>}
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

/**
 * Compact source-control outcome with raw Git output available on demand. These
 * notices linger until the next operation/session or until explicitly dismissed.
 */
function ScmOpNotice({
  status,
  title,
  summary,
  details,
  onDismiss,
}: {
  status: "success" | "error";
  title: string;
  summary: string;
  details: string;
  onDismiss: () => void;
}) {
  const { t } = useTranslation();
  const trimmedDetails = details.trim();
  const showDetails = trimmedDetails.length > 0 && trimmedDetails !== summary.trim();
  return (
    <div
      className={`scm-op-notice scm-op-${status}`}
      role={status === "error" ? "alert" : "status"}
      aria-live={status === "error" ? "assertive" : "polite"}
    >
      <span className="scm-op-status" aria-hidden="true">
        {status === "success" ? "✓" : "!"}
      </span>
      <div className="scm-op-copy">
        <div className="scm-op-title">{title}</div>
        <div className="scm-op-summary">{summary}</div>
        {showDetails && (
          <details className="scm-op-details">
            <summary>{t("rightPanel.showGitDetails")}</summary>
            <pre>{trimmedDetails}</pre>
          </details>
        )}
      </div>
      <button
        className="scm-op-dismiss"
        onClick={onDismiss}
        title={t("common.dismiss")}
        aria-label={t("common.dismiss")}
      >
        ×
      </button>
    </div>
  );
}

function scmErrorDetails(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Keep the actionable first line visible; the complete command output stays expandable. */
function scmErrorSummary(error: unknown): string {
  const details = scmErrorDetails(error).trim();
  const firstLine = details.split(/\r?\n/, 1)[0] || details;
  return firstLine.length > 180 ? `${firstLine.slice(0, 177)}…` : firstLine;
}

function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="field">
      <div className="field-label">{label}</div>
      <div className={"field-value" + (mono ? " mono" : "")}>{value}</div>
    </div>
  );
}
