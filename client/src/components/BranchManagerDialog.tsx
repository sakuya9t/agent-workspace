import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, BranchOverview, Session } from "../api";
import { daemonLabel, localTarget, targetOf, useConnStore } from "../connectionStore";
import { useDaemonStates } from "../useDaemons";
import { useUiStore } from "../store";
import { relTime } from "../i18n/time";

/**
 * Workspace-level git branch management. Opened from the (i) icon on a Git
 * workspace row: for every local branch it shows the sessions attached to it,
 * where it is based (the same base commit the right panel shows) and how many of
 * its commits are merged nowhere else — then lets the user merge, rebase, or
 * delete branches. All git work happens on the daemon; this only renders it.
 */
export function BranchManagerDialog() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showBranchManager);
  const setShow = useUiStore((s) => s.setShowBranchManager);
  const ctx = useUiStore((s) => s.branchManagerCtx);
  const daemons = useConnStore((s) => s.daemons);
  const states = useDaemonStates();

  const daemon = daemons.find((d) => d.id === ctx?.daemonId);
  const conn = daemon ? targetOf(daemon) : localTarget();
  const wsId = ctx?.workspaceId ?? "";
  const bundle = states.find((s) => s.daemon.id === ctx?.daemonId)?.data;
  const workspace = bundle?.workspaces.find((w) => w.id === wsId);
  const sessionsById = new Map<string, Session>(
    (bundle?.sessions ?? []).map((s) => [s.id, s]),
  );

  const overview = useQuery({
    queryKey: ["wsBranches", conn.baseUrl, wsId],
    queryFn: () => api.workspaceBranchOverview(conn, wsId),
    enabled: show && !!wsId,
    refetchInterval: 4000,
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["wsBranches", conn.baseUrl, wsId] });
    qc.invalidateQueries({ queryKey: ["daemon"] });
  };

  const del = useMutation({
    mutationFn: ({ branch, force }: { branch: string; force?: boolean }) =>
      api.deleteWorkspaceBranch(conn, wsId, branch, force ?? false),
    onSuccess: invalidate,
  });
  const merge = useMutation({
    mutationFn: ({ source, target }: { source: string; target: string }) =>
      api.mergeWorkspaceBranches(conn, wsId, source, target),
    onSuccess: invalidate,
  });
  const rebase = useMutation({
    mutationFn: ({ branch, onto }: { branch: string; onto: string }) =>
      api.rebaseWorkspaceBranch(conn, wsId, branch, onto),
    onSuccess: invalidate,
  });

  // Deleting an unmerged branch is guarded server-side with a 409; the first
  // click confirms the delete, the 409 escalates to name what would be lost.
  const onDelete = async (branch: string) => {
    if (!confirm(t("branchManager.confirmDelete", { branch }))) return;
    try {
      await del.mutateAsync({ branch });
    } catch (e) {
      if ((e as { status?: number }).status === 409) {
        if (confirm(t("branchManager.confirmDeleteForce", { message: (e as Error).message }))) {
          await del.mutateAsync({ branch, force: true }).catch(() => {});
        }
      }
    }
  };

  if (!show) return null;

  const busy = del.isPending || merge.isPending || rebase.isPending;
  const err = del.error || merge.error || rebase.error;
  const branches = overview.data?.branches ?? [];
  const allNames = branches.map((b) => b.name);

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal branch-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">
          <span>
            {t("branchManager.title")}
            {workspace && <span className="dim"> · {workspace.name}</span>}
          </span>
          <button className="btn tiny" onClick={() => setShow(false)}>
            {t("common.close")}
          </button>
        </div>

        {overview.isLoading && <div className="dim">{t("branchManager.loading")}</div>}
        {overview.error && <div className="error">{String(overview.error)}</div>}
        {overview.data && !overview.data.is_git && (
          <div className="dim">{t("branchManager.notGit")}</div>
        )}
        {err && <div className="error">{String(err)}</div>}

        {overview.data?.is_git && (
          <div className="branch-list">
            {branches.length === 0 && (
              <div className="dim">{t("branchManager.noBranches")}</div>
            )}
            {branches.map((b) => (
              <BranchRow
                key={b.name}
                b={b}
                others={allNames.filter((n) => n !== b.name)}
                sessionsById={sessionsById}
                busy={busy}
                onMerge={(source, target) => merge.mutate({ source, target })}
                onRebase={(branch, onto) => rebase.mutate({ branch, onto })}
                onDelete={onDelete}
              />
            ))}
          </div>
        )}

        <div className="dim small branch-foot">
          {t("branchManager.foot", { daemon: daemon && daemonLabel(daemon) })}
        </div>
      </div>
    </div>
  );
}

function BranchRow({
  b,
  others,
  sessionsById,
  busy,
  onMerge,
  onRebase,
  onDelete,
}: {
  b: BranchOverview;
  others: string[];
  sessionsById: Map<string, Session>;
  busy: boolean;
  onMerge: (source: string, target: string) => void;
  onRebase: (branch: string, onto: string) => void;
  onDelete: (branch: string) => void;
}) {
  const { t } = useTranslation();
  const [picker, setPicker] = useState<null | "merge" | "rebase">(null);
  const [sel, setSel] = useState("");

  const titles = b.session_ids
    .map((id) => sessionsById.get(id)?.title || id.slice(0, 8))
    .join(", ");
  const noTargets = others.length === 0;

  return (
    <div className="branch-row">
      <div className="branch-head">
        <span className="branch-name mono">{b.name}</span>
        {b.is_current && (
          <span className="branch-tag current">{t("branchManager.current")}</span>
        )}
        {b.checked_out_path && !b.is_current && (
          <span className="branch-tag" title={b.checked_out_path}>
            {t("branchManager.checkedOut")}
          </span>
        )}
        {b.owns_branch && (
          <span className="branch-tag" title={t("branchManager.ownedTitle")}>
            {t("branchManager.owned")}
          </span>
        )}
        {b.unmerged_commits > 0 && (
          <span className="branch-tag warn" title={t("branchManager.unmergedTitle")}>
            {t("branchManager.unmerged", { count: b.unmerged_commits })}
          </span>
        )}
      </div>

      <div className="branch-meta">
        <span className="dim" title={titles || undefined}>
          {b.session_ids.length === 0
            ? t("branchManager.noSessions")
            : b.live_count > 0
              ? t("branchManager.sessionsLive", {
                  count: b.session_ids.length,
                  live: b.live_count,
                })
              : t("branchManager.sessions", { count: b.session_ids.length })}
        </span>
        {b.base ? (
          <span className="branch-base">
            {t("branchManager.aheadOfBase", { count: b.base.ahead, short: b.base.short })}
            {b.base.refs.length === 0 && (
              <span className="branch-tag warn" title={t("branchManager.staleBaseTitle")}>
                {t("branchManager.staleBase")}
              </span>
            )}
          </span>
        ) : (
          <span className="dim">{t("branchManager.noBase")}</span>
        )}
      </div>

      {b.last_commit && (
        <div className="branch-commit dim mono" title={b.last_commit.subject}>
          {b.last_commit.short} · {b.last_commit.subject}
          {b.last_commit.timestamp > 0 && (
            <span> · {relTime(b.last_commit.timestamp * 1000)}</span>
          )}
        </div>
      )}

      <div className="branch-actions">
        {picker === null ? (
          <>
            <button
              className="btn tiny"
              disabled={noTargets || busy}
              title={noTargets ? t("branchManager.noOtherBranches") : undefined}
              onClick={() => {
                setPicker("merge");
                setSel("");
              }}
            >
              {t("branchManager.mergeInto")}
            </button>
            <button
              className="btn tiny"
              disabled={noTargets || busy}
              title={noTargets ? t("branchManager.noOtherBranches") : undefined}
              onClick={() => {
                setPicker("rebase");
                setSel("");
              }}
            >
              {t("branchManager.rebaseOnto")}
            </button>
            <button
              className="btn tiny danger"
              disabled={busy || !!b.checked_out_path}
              title={b.checked_out_path ? t("branchManager.deleteBlocked") : undefined}
              onClick={() => onDelete(b.name)}
            >
              {t("branchManager.delete")}
            </button>
          </>
        ) : (
          <div className="branch-picker">
            <span className="dim small">
              {picker === "merge"
                ? t("branchManager.mergeLabel")
                : t("branchManager.rebaseLabel")}
            </span>
            <select className="input" value={sel} onChange={(e) => setSel(e.target.value)}>
              <option value="">{t("branchManager.pickTarget")}</option>
              {others.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
            <button
              className="btn tiny primary"
              disabled={!sel || busy}
              onClick={() => {
                if (picker === "merge") onMerge(b.name, sel);
                else onRebase(b.name, sel);
                setPicker(null);
              }}
            >
              {t("common.go")}
            </button>
            <button className="btn tiny" onClick={() => setPicker(null)}>
              {t("common.cancel")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
