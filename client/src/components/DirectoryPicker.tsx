import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { Target } from "../connectionStore";

interface Props {
  target: Target;
  initialPath?: string;
  title?: string;
  onPick: (path: string) => void;
  onClose: () => void;
}

/**
 * Browses the daemon host's filesystem so the user can pick a directory
 * without typing the full path. The working directory lives on the server,
 * so this uses the daemon's /api/fs/list rather than a native file dialog.
 */
export function DirectoryPicker({ target, initialPath, title, onPick, onClose }: Props) {
  // `path` empty means "let the daemon default to home".
  const [path, setPath] = useState(initialPath ?? "");
  const [showHidden, setShowHidden] = useState(false);
  const [manual, setManual] = useState(initialPath ?? "");

  const { data, error, isFetching } = useQuery({
    queryKey: ["fs", target.baseUrl, path, showHidden],
    queryFn: () => api.fsList(target, path, showHidden),
    retry: false,
  });

  // Keep the editable path box in sync with wherever we navigated.
  useEffect(() => {
    if (data?.path) setManual(data.path);
  }, [data?.path]);

  const current = data?.path ?? path;

  return (
    <div
      className="modal-backdrop"
      onClick={(e) => {
        // Nested inside the new-session dialog's backdrop; don't let the click
        // bubble up and close the parent dialog too.
        e.stopPropagation();
        onClose();
      }}
    >
      <div className="modal picker-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">
          <span>{title ?? "Select directory"}</span>
          <button className="btn tiny" onClick={onClose}>
            close
          </button>
        </div>

        <div className="picker-path-row">
          <input
            className="input mono"
            value={manual}
            spellCheck={false}
            onChange={(e) => setManual(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") setPath(manual);
            }}
            placeholder="/absolute/path"
          />
          <button className="btn" onClick={() => setPath(manual)}>
            Go
          </button>
        </div>

        <div className="picker-toolbar">
          <button
            className="btn tiny"
            disabled={!data?.parent}
            onClick={() => data?.parent && setPath(data.parent)}
          >
            ↑ Up
          </button>
          <button className="btn tiny" onClick={() => setPath("")}>
            ~ Home
          </button>
          <label className="checkbox small">
            <input
              type="checkbox"
              checked={showHidden}
              onChange={(e) => setShowHidden(e.target.checked)}
            />
            hidden
          </label>
          {isFetching && <span className="dim small">loading…</span>}
        </div>

        {error && <div className="error">{String(error)}</div>}

        <div className="picker-list">
          {data?.entries.length === 0 && (
            <div className="dim small">No subdirectories.</div>
          )}
          {data?.entries.map((e) => (
            <div
              key={e.path}
              className="picker-entry"
              onDoubleClick={() => setPath(e.path)}
              onClick={() => setManual(e.path)}
              title="Double-click to open"
            >
              <span className="picker-icon">{e.is_git ? "◆" : "▸"}</span>
              <span className="mono">{e.name}</span>
              {e.is_git && <span className="git-tag">git</span>}
            </div>
          ))}
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={!current}
            onClick={() => onPick(current)}
          >
            Use this folder
          </button>
        </div>
      </div>
    </div>
  );
}
