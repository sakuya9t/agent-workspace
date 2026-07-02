import { useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { Target } from "../connectionStore";

interface Props {
  target: Target;
  sessionId: string;
  path: string;
  untracked: boolean;
  onClose: () => void;
}

/**
 * Read-only unified-diff viewer. Line-colored for MVP; a CodeMirror-based
 * side-by-side viewer is the next iteration (Phase 4).
 */
export function DiffModal({ target, sessionId, path, untracked, onClose }: Props) {
  const { data, error, isLoading } = useQuery({
    queryKey: ["diff", target.baseUrl, sessionId, path, untracked],
    queryFn: () => api.scmDiff(target, sessionId, path, untracked),
    retry: false,
  });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal diff-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">
          <span className="mono">{path}</span>
          <button className="btn tiny" onClick={onClose}>
            close
          </button>
        </div>
        <div className="diff-view mono">
          {isLoading && <div className="dim">Loading diff…</div>}
          {error && <div className="error">{String(error)}</div>}
          {data !== undefined && data.length === 0 && (
            <div className="dim">No changes to display.</div>
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
