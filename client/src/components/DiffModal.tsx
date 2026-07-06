import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { Target } from "../connectionStore";

interface Props {
  target: Target;
  sessionId: string;
  path: string;
  untracked: boolean;
  /** When set, show the file's diff as introduced by this commit. */
  commit?: string;
  onClose: () => void;
}

/**
 * Read-only unified-diff viewer. Line-colored for MVP; a CodeMirror-based
 * side-by-side viewer is the next iteration (Phase 4).
 */
export function DiffModal({ target, sessionId, path, untracked, commit, onClose }: Props) {
  const { t } = useTranslation();
  const { data, error, isLoading } = useQuery({
    queryKey: ["diff", target.baseUrl, sessionId, path, untracked, commit ?? null],
    queryFn: () => api.scmDiff(target, sessionId, path, untracked, commit),
    retry: false,
  });

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        // Close only when the press *starts* on the backdrop itself. Resizing
        // the dialog begins with a mousedown on its corner grip; releasing that
        // drag over the backdrop fires a click on the backdrop (the nearest
        // common ancestor of the press and release), which would otherwise
        // close the dialog mid-resize. Guarding on the press target avoids that.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal diff-modal">
        <div className="modal-title">
          <span className="mono">{path}</span>
          <button className="btn tiny" onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
        <div className="diff-view mono">
          {isLoading && <div className="dim">{t("diffModal.loading")}</div>}
          {error && <div className="error">{String(error)}</div>}
          {data !== undefined && data.length === 0 && (
            <div className="dim">{t("diffModal.noChanges")}</div>
          )}
          {data &&
            data.split("\n").map((line, i) => (
              <div key={i} className={"diff-line " + diffClass(line)}>
                {line || " "}
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}

function diffClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---")) return "meta";
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("diff ") || line.startsWith("index ")) return "meta";
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "del";
  return "";
}
