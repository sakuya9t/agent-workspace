import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, CommitFileStat } from "../api";
import { Target } from "../connectionStore";
import { relTime } from "../i18n/time";
import { shortPath } from "../paths";
import { DiffModal, DiffMode } from "./DiffModal";
import { FileViewActions } from "./FileViewActions";

interface Props {
  target: Target;
  sessionId: string;
  hash: string;
  onClose: () => void;
}

/**
 * Commit-detail popup mirroring the git-graph experience: subject, author,
 * absolute + relative time, message body, total churn, and the per-file
 * +/- list. Each file opens either its diff at this commit or the whole file as
 * of it (a nested DiffModal, rendered as a sibling so its backdrop click stays
 * scoped to it).
 */
export function CommitModal({ target, sessionId, hash, onClose }: Props) {
  const { t } = useTranslation();
  const [fileTarget, setFileTarget] = useState<{
    file: CommitFileStat;
    mode: DiffMode;
  } | null>(null);
  const { data, error, isLoading } = useQuery({
    queryKey: ["commit", target.baseUrl, sessionId, hash],
    queryFn: () => api.scmCommit(target, sessionId, hash),
    retry: false,
  });

  return (
    <>
      <div
        className="modal-backdrop"
        onMouseDown={(e) => {
          // Close only when the press *starts* on the backdrop itself, so a drag
          // that merely ends on it (e.g. selecting text in the message body and
          // releasing outside the dialog) can't dismiss it. Same guard as DiffModal.
          if (e.target === e.currentTarget) onClose();
        }}
      >
        <div className="modal commit-modal">
          <div className="modal-title">
            <span className="commit-modal-subject">
              {data?.subject ||
                (isLoading ? t("commitModal.loading") : t("rightPanel.noMessage"))}
            </span>
            <button className="btn tiny" onClick={onClose}>
              {t("common.close")}
            </button>
          </div>

          {isLoading && <div className="dim">{t("commitModal.loading")}</div>}
          {error && <div className="error">{String(error)}</div>}

          {data && (
            <div className="commit-detail">
              <div className="commit-detail-meta">
                <span className="mono">{data.short}</span>
                <span className="dim">
                  {" · "}
                  {data.author}
                  {data.email ? ` <${data.email}>` : ""}
                </span>
              </div>
              <div className="commit-detail-meta dim">
                {absTime(data.timestamp)} · {relTime(data.timestamp * 1000)}
              </div>
              {data.parents.length > 0 && (
                <div className="commit-detail-meta dim mono">
                  {t("commitModal.parents")}:{" "}
                  {data.parents.map((p) => p.slice(0, 7)).join(", ")}
                </div>
              )}

              {data.body && <pre className="commit-detail-body">{data.body}</pre>}

              <div className="commit-detail-stat">
                <span>{t("commitModal.filesChanged", { count: data.files.length })}</span>
                {data.additions > 0 && <span className="stat-add">+{data.additions}</span>}
                {data.deletions > 0 && <span className="stat-del">-{data.deletions}</span>}
              </div>

              <div className="commit-files">
                {data.files.length === 0 && (
                  <div className="dim small">{t("commitModal.noFiles")}</div>
                )}
                {data.files.map((f) => (
                  <div
                    key={f.path}
                    className="commit-file"
                    onClick={() => setFileTarget({ file: f, mode: "diff" })}
                    title={
                      f.orig_path
                        ? t("commitModal.renamed", { from: f.orig_path, to: f.path })
                        : t("rightPanel.viewDiff")
                    }
                  >
                    <span className="commit-file-nums mono">
                      {f.additions === null ? (
                        <span className="dim">{t("commitModal.binary")}</span>
                      ) : (
                        <>
                          <span className="stat-add">+{f.additions}</span>
                          <span className="stat-del">-{f.deletions}</span>
                        </>
                      )}
                    </span>
                    <span className="mono commit-file-path">
                      {f.orig_path
                        ? t("commitModal.renamed", {
                            from: shortPath(f.orig_path),
                            to: shortPath(f.path),
                          })
                        : shortPath(f.path)}
                    </span>
                    <FileViewActions onOpen={(mode) => setFileTarget({ file: f, mode })} />
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {fileTarget && (
        <DiffModal
          target={target}
          sessionId={sessionId}
          path={fileTarget.file.path}
          untracked={false}
          commit={hash}
          mode={fileTarget.mode}
          onClose={() => setFileTarget(null)}
        />
      )}
    </>
  );
}

function absTime(unixSecs: number): string {
  try {
    return new Date(unixSecs * 1000).toLocaleString();
  } catch {
    return "";
  }
}
