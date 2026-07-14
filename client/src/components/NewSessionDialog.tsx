import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";
import { api, type PluginInfo } from "../api";
import { daemonLabel, localTarget, targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import { DirectoryPicker } from "./DirectoryPicker";

type SessionTarget = { kind: "workspace"; id: string } | { kind: "path" };

/**
 * The agents a session can be forked into. Forking hands a conversation to an
 * agent, so a shell (no conversation to give it, and it would run a seed prompt
 * as a script) and `custom_command` (unknown by definition) are not candidates.
 *
 * When none of these is installed on a host, its sessions cannot be forked and
 * the Fork action is disabled — see `canForkInto`.
 */
export const FORKABLE_AGENTS = ["claude", "codex", "opencode"];

/** Whether this host has any agent a session could be forked into. */
export function canForkInto(plugins: PluginInfo[] | undefined): boolean {
  return (plugins ?? []).some((p) => p.available && FORKABLE_AGENTS.includes(p.id));
}

export function NewSessionDialog() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showNewSession);
  const setShow = useUiStore((s) => s.setShowNewSession);
  const setActive = useUiStore((s) => s.setActive);
  const presetDaemonId = useUiStore((s) => s.newSessionDaemonId);
  const presetWorkspaceId = useUiStore((s) => s.newSessionWorkspaceId);
  const daemons = useConnStore((s) => s.daemons);

  const [daemonId, setDaemonId] = useState("local");
  const [pluginId, setPluginId] = useState("shell");
  const [targetState, setTarget] = useState<SessionTarget>({ kind: "path" });
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
  const [sameBranch, setSameBranch] = useState(false);

  // Fork mode. A fork inherits the origin's place, so the dialog collapses to the
  // two choices a fork actually has: which agent, and same branch or a new one.
  const forkSource = useUiStore((s) => s.forkSource);
  const isFork = forkSource != null;

  // Opened from a workspace's "+": daemon and workspace are fixed — derive them
  // straight from the presets (not state) so the lock can't be bypassed.
  const lockedWs = presetWorkspaceId != null;
  const effDaemonId = lockedWs && presetDaemonId ? presetDaemonId : daemonId;
  const target: SessionTarget = lockedWs
    ? { kind: "workspace", id: presetWorkspaceId }
    : targetState;

  const daemon = daemons.find((d) => d.id === effDaemonId) ?? daemons[0];
  const conn = daemon ? targetOf(daemon) : localTarget();

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
    // A fork defaults to the origin's own agent (the common case — and the only
    // one that can carry the whole conversation) and to a branch of its own,
    // which is the only choice that's safe while the origin is still running.
    setSameBranch(false);
    if (forkSource) setPluginId(forkSource.agentPluginId);
  }, [show, presetDaemonId, presetWorkspaceId, forkSource]);

  const { data: plugins } = useQuery({
    queryKey: ["plugins", conn.baseUrl],
    queryFn: () => api.listPlugins(conn),
    enabled: show,
  });

  // Only offer agents whose binary is installed on the selected host. The daemon
  // detects this per-host (`available`); `custom_command` has no binary to detect
  // but is always available since the user supplies the command.
  //
  // A fork narrows this to the coding agents: forking exists to hand a
  // *conversation* to an agent, and a shell or an arbitrary command has no way to
  // receive one. `FORKABLE_AGENTS` is also what gates the Fork action itself — if
  // none of them is installed, there is nothing to fork into.
  const shownPlugins = (plugins ?? []).filter((p) =>
    isFork
      ? p.available && FORKABLE_AGENTS.includes(p.id)
      : p.available || p.id === "custom_command",
  );

  // Keep the selection valid: if the current agent isn't offered on this host
  // (e.g. after switching daemons), fall back to the first one that is.
  //
  // In fork mode the fallback is the *origin's* agent, not merely the first in
  // the list. Opening the fork dialog always trips this branch — the default
  // `pluginId` is "shell", which fork mode never offers — so a plain
  // `shownPlugins[0]` here silently overwrites the origin's agent with whichever
  // agent happens to sort first, and every fork would quietly default to changing
  // agent (and so to a summary instead of the full conversation).
  useEffect(() => {
    if (shownPlugins.length && !shownPlugins.some((p) => p.id === pluginId)) {
      const origin = forkSource?.agentPluginId;
      const fallback =
        origin && shownPlugins.some((p) => p.id === origin) ? origin : shownPlugins[0].id;
      setPluginId(fallback);
    }
  }, [shownPlugins, pluginId, forkSource]);
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
        lines.push(t("newSession.removedWorktrees", { count: report.removed_worktrees.length }));
      if (report.deleted_branches.length)
        lines.push(t("newSession.deletedBranches", { count: report.deleted_branches.length }));
      if (report.skipped_dirty.length)
        lines.push(t("newSession.skippedDirty", { count: report.skipped_dirty.length }));
      if (report.skipped_unmerged.length)
        lines.push(t("newSession.skippedUnmerged", { count: report.skipped_unmerged.length }));
      if (!lines.length) {
        alert(t("newSession.nothingOrphaned"));
        return;
      }
      const skipped = report.skipped_dirty.length + report.skipped_unmerged.length;
      if (
        !v.force &&
        skipped > 0 &&
        confirm(lines.join("\n") + "\n\n" + t("newSession.forcePrompt"))
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

      // A fork inherits its origin's daemon, workspace and place, so none of the
      // resolution below applies: the daemon works all of that out from the
      // origin. This call can take tens of seconds — it may run an agent
      // headlessly to write the handoff brief — which is why the button says so.
      if (forkSource) {
        return api.forkSession(conn, forkSource.sessionId, {
          agent_plugin_id: pluginId,
          same_branch: sameBranch,
          options: effectiveOptions,
        });
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
      setActive({ daemonId: effDaemonId, sessionId: session.id });
      setShow(false);
      setCommand("");
    },
  });

  if (!show) return null;

  const selectedPlugin = plugins?.find((p) => p.id === pluginId);
  const isCustom = pluginId === "custom_command";

  const branchOk =
    !isolatedGit ||
    branchMode === "auto" ||
    (branchMode === "new" && branchName.trim().length > 0) ||
    (branchMode === "existing" && (existingBranch || defaultBranch).length > 0);

  // A fork carries the whole conversation only when it stays on the origin's
  // agent *and* that agent kept a resumable conversation. Anything else — a
  // different agent, or an origin whose conversation was never captured — carries
  // a written summary instead. Saying which up front matters: one is lossless and
  // instant, the other takes a moment and is a summary.
  const forkCarriesConversation =
    isFork && forkSource.hasConversation && pluginId === forkSource.agentPluginId;

  const canSubmit = isFork
    ? !create.isPending
    : (target.kind === "workspace" ? !!activeWs : cwd.trim().length > 0) &&
      (!isCustom || (command.trim().length > 0 && approve)) &&
      branchOk &&
      !create.isPending;

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">
          {isFork ? t("fork.title", { name: forkSource.title }) : t("newSession.title")}
        </div>

        {!isFork && (
          <>
            <label className="form-label">{t("newSession.daemonLabel")}</label>
            <select
              className="input"
              value={effDaemonId}
              disabled={lockedWs}
              onChange={(e) => setDaemonId(e.target.value)}
            >
              {daemons.map((d) => (
                <option key={d.id} value={d.id}>
                  {daemonLabel(d)}
                  {d.baseUrl ? ` (${d.baseUrl})` : ""}
                </option>
              ))}
            </select>
          </>
        )}

        <label className="form-label">{t("newSession.agentLabel")}</label>
        <select className="input" value={pluginId} onChange={(e) => setPluginId(e.target.value)}>
          {shownPlugins.map((p) => (
            <option key={p.id} value={p.id}>
              {p.display_name}
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
            {o.danger && <span className="danger-tag">{t("newSession.dangerous")}</span>}
          </label>
        ))}

        {isFork && (
          <>
            <label className="form-label">{t("fork.placeLabel")}</label>
            <label
              className={"checkbox" + (sameBranch && forkSource.live ? " danger" : "")}
              title={t("fork.sameBranchHelp")}
            >
              <input
                type="checkbox"
                checked={sameBranch}
                disabled={!forkSource.branch}
                onChange={(e) => setSameBranch(e.target.checked)}
              />
              <span>
                {forkSource.branch
                  ? t("fork.sameBranch", { branch: forkSource.branch })
                  : t("fork.noBranch")}
              </span>
            </label>
            <div className="dim small">
              {sameBranch
                ? t("fork.sameBranchHint")
                : t("fork.newBranchHint", { branch: forkSource.branch ?? "" })}
            </div>

            {/* Two live agents editing one directory will overwrite each other.
                We allow it — sometimes it is what you want — but never quietly. */}
            {sameBranch && forkSource.live && (
              <div className="warn small">{t("fork.liveSameBranchWarning")}</div>
            )}

            <div className="dim small">
              {forkCarriesConversation
                ? t("fork.carriesConversation")
                : t("fork.carriesSummary")}
            </div>
          </>
        )}

        {!isFork && !lockedWs && (
          <>
            <label className="form-label">{t("newSession.runInLabel")}</label>
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
                {t("newSession.segWorkspace")}
              </button>
              <button
                className={"seg-btn" + (target.kind === "path" ? " on" : "")}
                onClick={() => setTarget({ kind: "path" })}
              >
                {t("newSession.segDirectory")}
              </button>
            </div>
          </>
        )}

        {!isFork && target.kind === "path" && (
          <>
            <label className="form-label">
              {t("newSession.workingDirOn", { daemon: daemon && daemonLabel(daemon) })}
            </label>
            <div className="path-row">
              <input
                className="input mono"
                placeholder={t("newSession.cwdPlaceholder")}
                value={cwd}
                onChange={(e) => setCwd(e.target.value)}
              />
              <button className="btn" onClick={() => setPicking("cwd")}>
                {t("common.browse")}
              </button>
            </div>
            <div className="dim small">{t("newSession.registeredHint")}</div>
          </>
        )}

        {!isFork && target.kind === "workspace" && (
          <>
            <label className="form-label">{t("newSession.workspaceLabel")}</label>
            <select
              className="input"
              value={activeWs?.id ?? ""}
              disabled={lockedWs}
              onChange={(e) => setTarget({ kind: "workspace", id: e.target.value })}
            >
              <option value="" disabled>
                {workspaces?.length
                  ? t("newSession.selectPlaceholder")
                  : t("newSession.noWorkspaces")}
              </option>
              {workspaces?.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name} · {w.is_git ? t("common.git") : t("common.plain")}
                  {w.root_exists === false ? ` · ${t("common.missing")}` : ""}
                </option>
              ))}
            </select>
            {activeWs && (
              <div className="path-row">
                <div
                  className="dim small mono"
                  style={activeWs.root_exists === false ? { color: "#f7768e" } : undefined}
                >
                  {activeWs.root_path}
                  {activeWs.root_exists === false
                    ? `  · ${t("newSession.missingOnHost")}`
                    : ""}
                </div>
                {!lockedWs && (
                  <button
                    className="btn tiny"
                    title={t("newSession.removeWsTitle")}
                    disabled={removeWs.isPending}
                    onClick={() => {
                      if (confirm(t("newSession.confirmRemoveWs", { name: activeWs.name })))
                        removeWs.mutate(activeWs.id);
                    }}
                  >
                    {t("newSession.remove")}
                  </button>
                )}
              </div>
            )}
            {removeWs.error && <div className="error">{String(removeWs.error)}</div>}
            {activeWs && activeWs.is_git && (
              <div className="hint">
                <button
                  className="btn tiny"
                  disabled={cleanupWt.isPending}
                  title={t("newSession.cleanupTitle")}
                  onClick={() => {
                    if (confirm(t("newSession.confirmCleanup", { name: activeWs.name })))
                      cleanupWt.mutate({ id: activeWs.id, force: false });
                  }}
                >
                  {cleanupWt.isPending
                    ? t("newSession.cleaning")
                    : t("newSession.cleanupBtn")}
                </button>
              </div>
            )}
            {activeWs && activeWs.is_git && (
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={directCheckout}
                  onChange={(e) => setDirectCheckout(e.target.checked)}
                />
                {t("newSession.directCheckout")}
              </label>
            )}

            {isolatedGit && (
              <>
                <label className="form-label">{t("newSession.branchLabel")}</label>
                <div className="seg">
                  <button
                    className={"seg-btn" + (branchMode === "auto" ? " on" : "")}
                    onClick={() => setBranchMode("auto")}
                  >
                    {t("newSession.segAuto")}
                  </button>
                  <button
                    className={"seg-btn" + (branchMode === "new" ? " on" : "")}
                    onClick={() => setBranchMode("new")}
                  >
                    {t("newSession.segNewBranch")}
                  </button>
                  <button
                    className={"seg-btn" + (branchMode === "existing" ? " on" : "")}
                    onClick={() => setBranchMode("existing")}
                    disabled={branches.length === 0}
                  >
                    {t("newSession.segExisting")}
                  </button>
                </div>

                {branchMode === "auto" && (
                  <div className="dim small">
                    <Trans
                      i18nKey="newSession.autoHint"
                      components={{ mono: <span className="mono" /> }}
                      values={{ prefix: "asm-session/…", base: defaultBranch || "HEAD" }}
                    />
                  </div>
                )}

                {branchMode === "new" && (
                  <>
                    <input
                      className="input mono"
                      placeholder={t("newSession.branchPlaceholder")}
                      value={branchName}
                      onChange={(e) => setBranchName(e.target.value)}
                    />
                    <label className="form-label">{t("newSession.basedOnLabel")}</label>
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
                    <div className="dim small">{t("newSession.existingHint")}</div>
                  </>
                )}
              </>
            )}
            {activeWs && !activeWs.is_git && (
              <div className="hint">
                {t("newSession.plainFolder")}{" "}
                <button
                  className="btn tiny"
                  disabled={initGit.isPending}
                  onClick={() => initGit.mutate(activeWs.id)}
                >
                  {t("newSession.gitInitBtn")}
                </button>
              </div>
            )}

            {!lockedWs && (
              <div className="register-box">
                <div className="dim small">
                  {t("newSession.registerNewWs", { daemon: daemon && daemonLabel(daemon) })}
                </div>
                <input
                  className="input"
                  placeholder={t("newSession.namePlaceholder")}
                  value={wsName}
                  onChange={(e) => setWsName(e.target.value)}
                />
                <div className="path-row">
                  <input
                    className="input mono"
                    placeholder={t("newSession.wsPathPlaceholder")}
                    value={wsPath}
                    onChange={(e) => setWsPath(e.target.value)}
                  />
                  <button className="btn" onClick={() => setPicking("wsPath")}>
                    {t("common.browse")}
                  </button>
                </div>
                {registerWs.error && <div className="error">{String(registerWs.error)}</div>}
                <button
                  className="btn"
                  disabled={!wsName.trim() || !wsPath.trim() || registerWs.isPending}
                  onClick={() => registerWs.mutate()}
                >
                  {registerWs.isPending
                    ? t("newSession.registering")
                    : t("newSession.register")}
                </button>
              </div>
            )}
          </>
        )}

        {isCustom && (
          <>
            <label className="form-label">{t("newSession.commandLabel")}</label>
            <input
              className="input mono"
              placeholder={t("newSession.commandPlaceholder")}
              value={command}
              onChange={(e) => setCommand(e.target.value)}
            />
            <label className="checkbox">
              <input type="checkbox" checked={approve} onChange={(e) => setApprove(e.target.checked)} />
              {t("newSession.approveLabel")}
            </label>
          </>
        )}

        {create.error && <div className="error">{String(create.error)}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            {t("common.cancel")}
          </button>
          <button className="btn primary" disabled={!canSubmit} onClick={() => create.mutate()}>
            {isFork
              ? create.isPending
                ? // A fork that has to summarize runs an agent headlessly and can
                  // take tens of seconds. Say what it's doing rather than look hung.
                  forkCarriesConversation
                  ? t("fork.forking")
                  : t("fork.summarizing")
                : t("fork.start")
              : create.isPending
                ? t("newSession.starting")
                : t("newSession.start")}
          </button>
        </div>
      </div>

      {picking && (
        <DirectoryPicker
          target={conn}
          title={
            picking === "cwd"
              ? t("directoryPicker.selectWorkingDirectory")
              : t("directoryPicker.selectWorkspaceRoot")
          }
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
