import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import { DirectoryPicker } from "./DirectoryPicker";

type SessionTarget = { kind: "workspace"; id: string } | { kind: "path" };

export function NewSessionDialog() {
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showNewSession);
  const setShow = useUiStore((s) => s.setShowNewSession);
  const setActive = useUiStore((s) => s.setActive);
  const presetDaemonId = useUiStore((s) => s.newSessionDaemonId);
  const presetWorkspaceId = useUiStore((s) => s.newSessionWorkspaceId);
  const daemons = useConnStore((s) => s.daemons);

  const [daemonId, setDaemonId] = useState("local");
  const [pluginId, setPluginId] = useState("shell");
  const [target, setTarget] = useState<SessionTarget>({ kind: "path" });
  const [cwd, setCwd] = useState("");
  const [command, setCommand] = useState("");
  const [approve, setApprove] = useState(false);
  const [directCheckout, setDirectCheckout] = useState(false);
  const [wsName, setWsName] = useState("");
  const [wsPath, setWsPath] = useState("");
  const [picking, setPicking] = useState<null | "cwd" | "wsPath">(null);

  const daemon = daemons.find((d) => d.id === daemonId) ?? daemons[0];
  const conn = daemon ? targetOf(daemon) : { baseUrl: "", token: null };

  // Apply presets when the dialog opens.
  useEffect(() => {
    if (!show) return;
    if (presetDaemonId) setDaemonId(presetDaemonId);
    if (presetWorkspaceId) setTarget({ kind: "workspace", id: presetWorkspaceId });
  }, [show, presetDaemonId, presetWorkspaceId]);

  const { data: plugins } = useQuery({
    queryKey: ["plugins", conn.baseUrl],
    queryFn: () => api.listPlugins(conn),
    enabled: show,
  });
  const { data: workspaces } = useQuery({
    queryKey: ["workspaces", conn.baseUrl],
    queryFn: () => api.listWorkspaces(conn),
    enabled: show,
  });

  const registerWs = useMutation({
    mutationFn: () => api.addWorkspace(conn, wsName, wsPath),
    onSuccess: (w) => {
      qc.invalidateQueries({ queryKey: ["workspaces", conn.baseUrl] });
      qc.invalidateQueries({ queryKey: ["daemon"] });
      setTarget({ kind: "workspace", id: w.id });
      setWsName("");
      setWsPath("");
    },
  });

  const initGit = useMutation({
    mutationFn: (id: string) => api.initWorkspaceGit(conn, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces", conn.baseUrl] }),
  });

  const create = useMutation({
    mutationFn: () =>
      api.createSession(conn, {
        agent_plugin_id: pluginId,
        cwd: target.kind === "path" ? cwd : undefined,
        workspace_id: target.kind === "workspace" ? target.id : undefined,
        command: pluginId === "custom_command" ? command : undefined,
        approve_custom: approve,
        direct_checkout: directCheckout,
      }),
    onSuccess: (session) => {
      qc.invalidateQueries({ queryKey: ["daemon"] });
      setActive({ daemonId, sessionId: session.id });
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

        <label className="form-label">Daemon</label>
        <select className="input" value={daemonId} onChange={(e) => setDaemonId(e.target.value)}>
          {daemons.map((d) => (
            <option key={d.id} value={d.id}>
              {d.label}
              {d.baseUrl ? ` (${d.baseUrl})` : ""}
            </option>
          ))}
        </select>

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
            <label className="form-label">Working directory (on {daemon?.label})</label>
            <div className="path-row">
              <input
                className="input mono"
                placeholder="/home/you/project"
                value={cwd}
                onChange={(e) => setCwd(e.target.value)}
              />
              <button className="btn" onClick={() => setPicking("cwd")}>
                Browse…
              </button>
            </div>
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
            {selectedWs && <div className="dim small mono">{selectedWs.root_path}</div>}
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
              <div className="dim small">Register a new workspace on {daemon?.label}</div>
              <input
                className="input"
                placeholder="name"
                value={wsName}
                onChange={(e) => setWsName(e.target.value)}
              />
              <div className="path-row">
                <input
                  className="input mono"
                  placeholder="/absolute/path/on/that/host"
                  value={wsPath}
                  onChange={(e) => setWsPath(e.target.value)}
                />
                <button className="btn" onClick={() => setPicking("wsPath")}>
                  Browse…
                </button>
              </div>
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

      {picking && (
        <DirectoryPicker
          target={conn}
          title={picking === "cwd" ? "Select working directory" : "Select workspace root"}
          initialPath={picking === "cwd" ? cwd : wsPath}
          onPick={(p) => {
            if (picking === "cwd") setCwd(p);
            else setWsPath(p);
            setPicking(null);
          }}
          onClose={() => setPicking(null)}
        />
      )}
    </div>
  );
}
