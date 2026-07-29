import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { Target } from "../connectionStore";

/** Extensions the daemon can serve as an inline image (magic-byte sniffed). */
const IMAGE_EXT = /\.(png|jpe?g|gif|webp)$/i;
function isImagePath(path: string): boolean {
  return IMAGE_EXT.test(path);
}

/** Turn a fetched Blob into an object URL, revoking it on change/unmount. */
function useObjectUrl(blob: Blob | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!blob) {
      setUrl(null);
      return;
    }
    const u = URL.createObjectURL(blob);
    setUrl(u);
    return () => URL.revokeObjectURL(u);
  }, [blob]);
  return url;
}

/** What the viewer is showing: the change, or the file the change landed in. */
export type DiffMode = "diff" | "file";

interface Props {
  target: Target;
  sessionId: string;
  path: string;
  untracked: boolean;
  /** When set, show the file's diff as introduced by this commit. */
  commit?: string;
  /** Which view to open on; the user can switch once it's open. */
  mode?: DiffMode;
  onClose: () => void;
}

/**
 * Read-only viewer for one file, in two modes: the unified diff (what changed)
 * and the whole file (what it now says). Line-colored for MVP; a CodeMirror-based
 * side-by-side viewer is the next iteration (Phase 4).
 */
export function DiffModal({
  target,
  sessionId,
  path,
  untracked,
  commit,
  mode: initialMode = "diff",
  onClose,
}: Props) {
  const { t } = useTranslation();
  const isImage = isImagePath(path);
  // An image *is* its own whole-file view — the before/after preview already
  // shows the file rather than a description of it — so images stay single-mode.
  const [mode, setMode] = useState<DiffMode>(initialMode);

  // Text diff for everything else. Skipped for images: git only reports
  // "Binary files differ" (or dumps raw bytes), neither of which is useful.
  const { data, error, isLoading } = useQuery({
    queryKey: ["diff", target.baseUrl, sessionId, path, untracked, commit ?? null],
    queryFn: () => api.scmDiff(target, sessionId, path, untracked, commit),
    enabled: !isImage && mode === "diff",
    retry: false,
  });

  // Whole-file text. `after` is the file as it stands (working tree, or at the
  // commit); a file deleted by this change has none, so fall back to the version
  // it had before — the only content there is to read.
  const fileAfter = useQuery({
    queryKey: ["scmContent", target.baseUrl, sessionId, path, commit ?? null, "after"],
    queryFn: () => api.scmContent(target, sessionId, path, "after", commit),
    enabled: !isImage && mode === "file",
    retry: false,
  });
  const deleted = (fileAfter.error as { status?: number } | null)?.status === 404;
  const fileBefore = useQuery({
    queryKey: ["scmContent", target.baseUrl, sessionId, path, commit ?? null, "before"],
    queryFn: () => api.scmContent(target, sessionId, path, "before", commit),
    enabled: !isImage && mode === "file" && deleted && !untracked,
    retry: false,
  });
  const file = fileAfter.data ?? fileBefore.data;
  const fileLoading = fileAfter.isLoading || (deleted && fileBefore.isLoading);
  const fileError = file || fileLoading ? null : (fileBefore.error ?? fileAfter.error);

  // Images: fetch each side and show them as a before/after comparison. Each
  // Blob becomes an object URL so `<img>` can display it without threading an
  // auth header through the tag. The "before" side is skipped for untracked
  // (new) files, which have none; any side the server reports absent (404 — a
  // new file's before, a deleted file's after) simply drops from the view.
  const after = useQuery({
    queryKey: ["scmFile", target.baseUrl, sessionId, path, commit ?? null, "after"],
    queryFn: () => api.scmFile(target, sessionId, path, "after", commit),
    enabled: isImage,
    retry: false,
  });
  const before = useQuery({
    queryKey: ["scmFile", target.baseUrl, sessionId, path, commit ?? null, "before"],
    queryFn: () => api.scmFile(target, sessionId, path, "before", commit),
    enabled: isImage && !untracked,
    retry: false,
  });
  const afterUrl = useObjectUrl(after.data);
  const beforeUrl = useObjectUrl(before.data);
  const imageLoading = after.isLoading || (isImage && !untracked && before.isLoading);
  // Only a real problem once nothing loaded and nothing is still in flight.
  const imageError =
    !afterUrl && !beforeUrl && !imageLoading ? (after.error ?? before.error) : null;

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
          <span className="diff-modal-actions">
            {!isImage && (
              <span className="mode-toggle" role="group" aria-label={t("diffModal.modeGroup")}>
                <button
                  className={"btn tiny" + (mode === "diff" ? " active" : "")}
                  aria-pressed={mode === "diff"}
                  onClick={() => setMode("diff")}
                >
                  {t("diffModal.modeDiff")}
                </button>
                <button
                  className={"btn tiny" + (mode === "file" ? " active" : "")}
                  aria-pressed={mode === "file"}
                  onClick={() => setMode("file")}
                >
                  {t("diffModal.modeFile")}
                </button>
              </span>
            )}
            <button className="btn tiny" onClick={onClose}>
              {t("common.close")}
            </button>
          </span>
        </div>
        <div className={"diff-view mono" + (isImage ? " diff-image-view" : "")}>
          {isImage ? (
            <>
              {imageLoading && <div className="dim">{t("diffModal.loadingImage")}</div>}
              {imageError && <div className="error">{String(imageError)}</div>}
              {(beforeUrl || afterUrl) && (
                <div className="image-diff">
                  {beforeUrl && (
                    <figure className="image-side">
                      {afterUrl && <figcaption className="dim">{t("diffModal.before")}</figcaption>}
                      <img className="diff-image" src={beforeUrl} alt={t("diffModal.before")} />
                    </figure>
                  )}
                  {afterUrl && (
                    <figure className="image-side">
                      {beforeUrl && <figcaption className="dim">{t("diffModal.after")}</figcaption>}
                      <img className="diff-image" src={afterUrl} alt={t("diffModal.after")} />
                    </figure>
                  )}
                </div>
              )}
            </>
          ) : mode === "file" ? (
            <>
              {fileLoading && <div className="dim">{t("diffModal.loadingFile")}</div>}
              {fileError && <div className="error">{String(fileError)}</div>}
              {fileBefore.data && (
                <div className="dim file-note">{t("diffModal.showingBeforeDeletion")}</div>
              )}
              {file?.binary && <div className="dim">{t("diffModal.binaryFile")}</div>}
              {file && !file.binary && file.content.length === 0 && (
                <div className="dim">{t("diffModal.emptyFile")}</div>
              )}
              {file && !file.binary && file.content.length > 0 && (
                <FileLines content={file.content} />
              )}
              {file?.truncated && <div className="dim file-note">{t("diffModal.truncated")}</div>}
            </>
          ) : (
            <>
              {isLoading && <div className="dim">{t("diffModal.loading")}</div>}
              {error && <div className="error">{String(error)}</div>}
              {data !== undefined && data.length === 0 && (
                <div className="dim">{t("diffModal.noChanges")}</div>
              )}
              {data &&
                data.split("\n").map((line, i) => (
                  <div key={i} className={"diff-line " + diffClass(line)}>
                    {line || " "}
                  </div>
                ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * The file as numbered lines. A trailing newline ends the last line rather than
 * starting an empty one, so the count matches what an editor would show.
 */
function FileLines({ content }: { content: string }) {
  const lines = content.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return (
    <div className="file-lines">
      {lines.map((line, i) => (
        <div key={i} className="file-line">
          <span className="file-lineno">{i + 1}</span>
          <span className="file-code">{line || " "}</span>
        </div>
      ))}
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
