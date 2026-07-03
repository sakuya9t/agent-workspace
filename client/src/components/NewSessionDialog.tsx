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
  const [agentOptions, setAgentOptions] = useState<Record<string, boolean>>({});
  const [directCheckout, setDirectCheckout] = useState(false);
  const [branchMode, setBranchMode] = useState<"auto" | "new" | "existing">("auto");
  const [branchName, setBranchName] = useState("");
  const [baseRef, setBaseRef] = useState("");
  const [existingBranch, setExistingBranch] = useState("");
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
    setBranchMode("auto");
    setBranchName("");
    setBaseRef("");
    setExistingBranch("");
    setAgentOptions({});
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

  // Selected workspace + whether an isolated worktree (with a branch choice)
  // applies. Computed here (not after the early return) to keep hook order stable.
  const activeWs =
    target.kind === "workspace" ? workspaces?.find((w) => w.id === target.id) : undefined;
  const isolatedGit = !!activeWs && activeWs.is_git && !directCheckout;

  const { data: branchData } = useQuery({
    queryKey: ["branches", conn.baseUrl, activeWs?.id],
    queryFn: () => api.workspaceBranches(conn, activeWs!.id),
    enabled: show && isolatedGit,
  });
  const branches = branchData?.branches ?? [];
  const defaultBranch = branchData?.head ?? branches[0] ?? "";

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

  const removeWs = useMutation({
    mutationFn: (id: string) => api.removeWorkspace(conn, id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workspaces", conn.baseUrl] });
      qc.invalidateQueries({ queryKey: ["daemon"] });
      setTarget({ kind: "workspace", id: "" });
    },
  });

  const cleanupWt = useMutation({
    mutationFn: (v: { id: string; force: boolean }) =>
      api.cleanupWorktrees(conn, v.id, v.force),
    onSuccess: (report, v) => {
      qc.invalidateQueries({ queryKey: ["workspaces", conn.baseUrl] });
      const lines: string[] = [];
      if (report.removed_worktrees.length)
        lines.push(`Removed ${report.removed_worktrees.length} orphaned worktree(s).`);
      if (report.deleted_branches.length)
        lines.push(`Deleted ${report.deleted_branches.length} orphaned branch(es).`);
      if (report.skipped_dirty.length)
        lines.push(`Skipped ${report.skipped_dirty.length} worktree(s) with uncommitted changes.`);
      if (report.skipped_unmerged.length)
        lines.push(`Skipped ${report.skipped_unmerged.length} branch(es) with unmerged commits.`);
      if (!lines.length) {
        alert("Nothing orphaned to clean up.");
        return;
      }
      const skipped = report.skipped_dirty.length + report.skipped_unmerged.length;
      if (
        !v.force &&
        skipped > 0 &&
        confirm(
          lines.join("\n") +
            "\n\nForce-remove the skipped ones too? This DISCARDS uncommitted changes and unmerged commits.",
        )
      ) {
        cleanupWt.mutate({ id: v.id, force: true });
      } else {
        alert(lines.join("\n"));
      }
    },
    onError: (e) => alert(String(e)),
  });

  const create = useMutation({
    mutationFn: async () => {
      const plugin = plugins?.find((p) => p.id === pluginId);
      const effectiveOptions: Record<string, boolean> = {};
      for (const o of plugin?.options ?? []) {
        effectiveOptions[o.key] = agentOptions[o.key] ?? o.default;
      }

      // Resolve where to run. A workspace is used directly; a raw directory is
      // auto-registered as a workspace (reusing one with the same root) so it is
      // allowlisted rather than rejected, then run in place (no worktree).
      let workspaceId: string;
      let useDirect = directCheckout;
      if (target.kind === "workspace") {
        workspaceId = target.id;
      } else {
        const path = cwd.trim();
        const existing = workspaces?.find((w) => w.root_path === path);
        const ws = existing ?? (await api.addWorkspace(conn, dirLabel(path), path));
        workspaceId = ws.id;
        useDirect = true;
        qc.invalidateQueries({ queryKey: ["workspaces", conn.baseUrl] });
      }

      const base = baseRef || defaultBranch;
      const existing = existingBranch || defaultBranch;
      const branchArgs =
        isolatedGit && branchMode === "new"
          ? { branch: branchName.trim(), create_branch: true, base_ref: base || undefined }
          : isolatedGit && branchMode === "existing"
            ? { branch: existing, create_branch: false }
            : {};
      return api.createSession(conn, {
        agent_plugin_id: pluginId,
        workspace_id: workspaceId,
        command: pluginId === "custom_command" ? command : undefined,
        approve_custom: approve,
        direct_checkout: useDirect,
        options: effectiveOptions,
        ...branchArgs,
      });
    },
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
  const selectedWs = activeWs;

  const branchOk =
    !isolatedGit ||
    branchMode === "auto" ||
    (branchMode === "new" && branchName.trim().length > 0) ||
    (branchMode === "existing" && (existingBranch || defaultBranch).length > 0);

  const canSubmit =
    (target.kind === "workspace" ? !!selectedWs : cwd.trim().length > 0) &&
    (!isCustom || (command.trim().length > 0 && approve)) &&
    branchOk &&
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

        {selectedPlugin?.options?.map((o) => (
          <label
            key={o.key}
            className={"checkbox" + (o.danger ? " danger" : "")}
            title={o.description}
          >
            <input
              type="checkbox"
              checked={agentOptions[o.key] ?? o.default}
              onChange={(e) =>
                setAgentOptions((prev) => ({ ...prev, [o.key]: e.target.checked }))
              }
            />
            <span>{o.label}</span>
            {o.danger && <span className="danger-tag">dangerous</span>}
          </label>
        ))}

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
            <div className="dim small">
              Registered as a workspace on first use so it stays on the allowlist;
              remove it later from its workspace node in the sidebar.
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
                  {w.root_exists === false ? " · missing" : ""}
                </option>
              ))}
            </select>
            {selectedWs && (
              <div className="path-row">
                <div
                  className="dim small mono"
                  style={selectedWs.root_exists === false ? { color: "#f7768e" } : undefined}
                >
                  {selectedWs.root_path}
                  {selectedWs.root_exists === false ? "  · missing on host" : ""}
                </div>
                <button
                  className="btn tiny"
                  title="Unregister this workspace (files are left intact)"
                  disabled={removeWs.isPending}
                  onClick={() => {
                    if (confirm(`Remove workspace "${selectedWs.name}"?`)) removeWs.mutate(selectedWs.id);
                  }}
                >
                  Remove
                </button>
              </div>
            )}
            {removeWs.error && <div className="error">{String(removeWs.error)}</div>}
            {selectedWs && selectedWs.is_git && (
              <div className="hint">
                <button
                  className="btn tiny"
                  disabled={cleanupWt.isPending}
                  title="Remove worktrees/branches left in this repo by throwaway or other daemons (that no session here owns)"
                  onClick={() => {
                    if (
                      confirm(
                        `Scan "${selectedWs.name}" for orphaned asm-session worktrees/branches and remove ones no session on this daemon owns?`,
                      )
                    )
                      cleanupWt.mutate({ id: selectedWs.id, force: false });
                  }}
                >
                  {cleanupWt.isPending ? "Cleaning…" : "Clean up orphaned worktrees"}
                </button>
              </div>
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

            {isolatedGit && (
              <>
                <label className="form-label">Branch</label>
                <div className="seg">
                  <button
                    className={"seg-btn" + (branchMode === "auto" ? " on" : "")}
                    onClick={() => setBranchMode("auto")}
                  >
                    Auto
                  </button>
                  <button
                    className={"seg-btn" + (branchMode === "new" ? " on" : "")}
                    onClick={() => setBranchMode("new")}
                  >
                    New branch
                  </button>
                  <button
                    className={"seg-btn" + (branchMode === "existing" ? " on" : "")}
                    onClick={() => setBranchMode("existing")}
                    disabled={branches.length === 0}
                  >
                    Existing
                  </button>
                </div>

                {branchMode === "auto" && (
                  <div className="dim small">
                    A fresh <span className="mono">asm-session/…</span> branch is created off{" "}
                    <span className="mono">{defaultBranch || "HEAD"}</span>.
                  </div>
                )}

                {branchMode === "new" && (
                  <>
                    <input
                      className="input mono"
                      placeholder="feature/my-branch"
                      value={branchName}
                      onChange={(e) => setBranchName(e.target.value)}
                    />
                    <label className="form-label">Based on</label>
                    <select
                      className="input"
                      value={baseRef || defaultBranch}
                      onChange={(e) => setBaseRef(e.target.value)}
                    >
                      {branches.map((b) => (
                        <option key={b} value={b}>
                          {b}
                        </option>
                      ))}
                    </select>
                  </>
                )}

                {branchMode === "existing" && (
                  <>
                    <select
                      className="input"
                      value={existingBranch || defaultBranch}
                      onChange={(e) => setExistingBranch(e.target.value)}
                    >
                      {branches.map((b) => (
                        <option key={b} value={b}>
                          {b}
                        </option>
                      ))}
                    </select>
                    <div className="dim small">
                      Checked out in a new worktree. A branch already checked out
                      elsewhere can't be reused.
                    </div>
                  </>
                )}
              </>
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

/** Last path segment, used as the auto-registered workspace name. */
function dirLabel(p: string): string {
  const parts = p.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : p || "dir";
}
