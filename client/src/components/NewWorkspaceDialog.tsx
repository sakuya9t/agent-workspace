import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { daemonLabel, targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import { DirectoryPicker } from "./DirectoryPicker";

/** Registers a directory on a specific daemon as a workspace. */
export function NewWorkspaceDialog() {
  const { t } = useTranslation();
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
        <div className="modal-title">
          {t("newWorkspace.title", { daemon: daemon && daemonLabel(daemon) })}
        </div>

        <label className="form-label">
          {t("newWorkspace.rootLabel", { daemon: daemon && daemonLabel(daemon) })}
        </label>
        <div className="path-row">
          <input
            className="input mono"
            placeholder={t("newWorkspace.pathPlaceholder")}
            value={path}
            onChange={(e) => setPath(e.target.value)}
          />
          <button className="btn" onClick={() => setPicking(true)}>
            {t("common.browse")}
          </button>
        </div>

        <label className="form-label">{t("newWorkspace.nameLabel")}</label>
        <input
          className="input"
          placeholder={dirLabel(path.trim()) || t("newWorkspace.namePlaceholder")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <div className="dim small">{t("newWorkspace.worktreeHint")}</div>

        {register.error && <div className="error">{String(register.error)}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            {t("common.cancel")}
          </button>
          <button
            className="btn primary"
            disabled={!path.trim() || register.isPending}
            onClick={() => register.mutate()}
          >
            {register.isPending ? t("newWorkspace.creating") : t("newWorkspace.create")}
          </button>
        </div>
      </div>

      {picking && (
        <DirectoryPicker
          target={conn}
          title={t("directoryPicker.selectWorkspaceRoot")}
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
