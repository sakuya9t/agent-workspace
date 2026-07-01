import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { useUiStore } from "../store";

type Target = { kind: "workspace"; id: string } | { kind: "path" };

export function NewSessionDialog() {
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showNewSession);
  const setShow = useUiStore((s) => s.setShowNewSession);
  const setActive = useUiStore((s) => s.setActive);

  const { data: plugins } = useQuery({
    queryKey: ["plugins"],
    queryFn: api.listPlugins,
    enabled: show,
  });
  const { data: workspaces } = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.listWorkspaces,
    enabled: show,
  });

  const [pluginId, setPluginId] = useState("shell");
  const [target, setTarget] = useState<Target>({ kind: "path" });
  const [cwd, setCwd] = useState("");
  const [command, setCommand] = useState("");
  const [approve, setApprove] = useState(false);
  const [directCheckout, setDirectCheckout] = useState(false);

  // Inline workspace registration.
  const [wsName, setWsName] = useState("");
  const [wsPath, setWsPath] = useState("");

  const registerWs = useMutation({
    mutationFn: () => api.addWorkspace(wsName, wsPath),
    onSuccess: (w) => {
      qc.invalidateQueries({ queryKey: ["workspaces"] });
      setTarget({ kind: "workspace", id: w.id });
      setWsName("");
      setWsPath("");
    },
  });

  const initGit = useMutation({
    mutationFn: (id: string) => api.initWorkspaceGit(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });

  const create = useMutation({
    mutationFn: () =>
      api.createSession({
        agent_plugin_id: pluginId,
        cwd: target.kind === "path" ? cwd : undefined,
        workspace_id: target.kind === "workspace" ? target.id : undefined,
        command: pluginId === "custom_command" ? command : undefined,
        approve_custom: approve,
        direct_checkout: directCheckout,
      }),
    onSuccess: (session) => {
      qc.invalidateQueries({ queryKey: ["sessions"] });
      setActive(session.id);
      setShow(false);
      setCommand("");
    },
  });

  if (!show) return null;

  const selectedPlugin = plugins?.find((p) => p.id === pluginId);
  const isCustom = pluginId === "custom_command";
  const selectedWs =
    target.kind === "workspace" ? workspaces?.find((w) => w.id === target.id) : undefined;

  const canSubmit =
    (target.kind === "workspace" ? !!selectedWs : cwd.trim().length > 0) &&
    (!isCustom || (command.trim().length > 0 && approve)) &&
    !create.isPending;

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">New session</div>

        <label className="form-label">Agent</label>
        <select className="input" value={pluginId} onChange={(e) => setPluginId(e.target.value)}>
          {plugins?.map((p) => (
            <option key={p.id} value={p.id} disabled={!p.supported_on_this_platform}>
              {p.display_name}
              {p.id !== "custom_command" && !p.available ? " (not installed)" : ""}
            </option>
          ))}
        </select>
        {selectedPlugin?.binary_path && (
          <div className="dim small mono">{selectedPlugin.binary_path}</div>
        )}

        <label className="form-label">Run in</label>
        <div className="seg">
          <button
            className={"seg-btn" + (target.kind === "workspace" ? " on" : "")}
            onClick={() =>
              setTarget(
                workspaces && workspaces[0]
                  ? { kind: "workspace", id: workspaces[0].id }
                  : { kind: "workspace", id: "" },
              )
            }
          >
            Workspace (isolated)
          </button>
          <button
            className={"seg-btn" + (target.kind === "path" ? " on" : "")}
            onClick={() => setTarget({ kind: "path" })}
          >
            Directory
          </button>
        </div>

        {target.kind === "path" && (
          <>
            <label className="form-label">Working directory (absolute path on server)</label>
            <input
              className="input mono"
              placeholder="/home/you/project"
              value={cwd}
              onChange={(e) => setCwd(e.target.value)}
            />
          </>
        )}

        {target.kind === "workspace" && (
          <>
            <label className="form-label">Workspace</label>
            <select
              className="input"
              value={selectedWs?.id ?? ""}
              onChange={(e) => setTarget({ kind: "workspace", id: e.target.value })}
            >
              <option value="" disabled>
                {workspaces?.length ? "Select…" : "No workspaces yet"}
              </option>
              {workspaces?.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name} {w.is_git ? "· git" : "· plain"}
                </option>
              ))}
            </select>
            {selectedWs && (
              <div className="dim small mono">{selectedWs.root_path}</div>
            )}
            {selectedWs && selectedWs.is_git && (
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={directCheckout}
                  onChange={(e) => setDirectCheckout(e.target.checked)}
                />
                Run in source checkout instead of an isolated worktree (override)
              </label>
            )}
            {selectedWs && !selectedWs.is_git && (
              <div className="hint">
                Plain folder — no isolated worktree.{" "}
                <button
                  className="btn tiny"
                  disabled={initGit.isPending}
                  onClick={() => initGit.mutate(selectedWs.id)}
                >
                  git init for change tracking
                </button>
              </div>
            )}

            <div className="register-box">
              <div className="dim small">Register a new workspace</div>
              <input
                className="input"
                placeholder="name"
                value={wsName}
                onChange={(e) => setWsName(e.target.value)}
              />
              <input
                className="input mono"
                placeholder="/absolute/path/on/server"
                value={wsPath}
                onChange={(e) => setWsPath(e.target.value)}
              />
              {registerWs.error && <div className="error">{String(registerWs.error)}</div>}
              <button
                className="btn"
                disabled={!wsName.trim() || !wsPath.trim() || registerWs.isPending}
                onClick={() => registerWs.mutate()}
              >
                {registerWs.isPending ? "Registering…" : "Register"}
              </button>
            </div>
          </>
        )}

        {isCustom && (
          <>
            <label className="form-label">Command</label>
            <input
              className="input mono"
              placeholder="my-tool --flag"
              value={command}
              onChange={(e) => setCommand(e.target.value)}
            />
            <label className="checkbox">
              <input type="checkbox" checked={approve} onChange={(e) => setApprove(e.target.checked)} />
              I approve running this arbitrary command
            </label>
          </>
        )}

        {create.error && <div className="error">{String(create.error)}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            Cancel
          </button>
          <button className="btn primary" disabled={!canSubmit} onClick={() => create.mutate()}>
            {create.isPending ? "Starting…" : "Start"}
          </button>
        </div>
      </div>
    </div>
  );
}
