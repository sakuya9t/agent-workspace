import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import { DirectoryPicker } from "./DirectoryPicker";

/** Registers a directory on a specific daemon as a workspace. */
export function NewWorkspaceDialog() {
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showNewWorkspace);
  const setShow = useUiStore((s) => s.setShowNewWorkspace);
  const daemonId = useUiStore((s) => s.newWorkspaceDaemonId);
  const daemons = useConnStore((s) => s.daemons);

  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    if (!show) return;
    setName("");
    setPath("");
  }, [show, daemonId]);

  const daemon = daemons.find((d) => d.id === daemonId) ?? daemons[0];
  const conn = daemon ? targetOf(daemon) : { baseUrl: "", token: null };

  const register = useMutation({
    mutationFn: () =>
      api.addWorkspace(conn, name.trim() || dirLabel(path.trim()), path.trim()),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workspaces", conn.baseUrl] });
      qc.invalidateQueries({ queryKey: ["daemon"] });
      setShow(false);
    },
  });

  if (!show) return null;

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">New workspace on {daemon?.label}</div>

        <label className="form-label">Root directory (on {daemon?.label})</label>
        <div className="path-row">
          <input
            className="input mono"
            placeholder="/absolute/path/on/that/host"
            value={path}
            onChange={(e) => setPath(e.target.value)}
          />
          <button className="btn" onClick={() => setPicking(true)}>
            Browse…
          </button>
        </div>

        <label className="form-label">Name</label>
        <input
          className="input"
          placeholder={dirLabel(path.trim()) || "name"}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <div className="dim small">
          Sessions started in this workspace run in isolated worktrees when it is
          a git repository.
        </div>

        {register.error && <div className="error">{String(register.error)}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={!path.trim() || register.isPending}
            onClick={() => register.mutate()}
          >
            {register.isPending ? "Creating…" : "Create"}
          </button>
        </div>
      </div>

      {picking && (
        <DirectoryPicker
          target={conn}
          title="Select workspace root"
          initialPath={path}
          onPick={(p) => {
            setPath(p);
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </div>
  );
}

/** Last path segment, used as the default workspace name. */
function dirLabel(p: string): string {
  const parts = p.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : "";
}
