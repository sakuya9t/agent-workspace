import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { useUiStore } from "../store";

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

  const [pluginId, setPluginId] = useState("shell");
  const [cwd, setCwd] = useState("");
  const [command, setCommand] = useState("");
  const [approve, setApprove] = useState(false);

  const create = useMutation({
    mutationFn: () =>
      api.createSession({
        agent_plugin_id: pluginId,
        cwd,
        command: pluginId === "custom_command" ? command : undefined,
        approve_custom: approve,
      }),
    onSuccess: (session) => {
      qc.invalidateQueries({ queryKey: ["sessions"] });
      setActive(session.id);
      setShow(false);
      setCommand("");
    },
  });

  if (!show) return null;

  const selected = plugins?.find((p) => p.id === pluginId);
  const isCustom = pluginId === "custom_command";
  const canSubmit =
    cwd.trim().length > 0 &&
    (!isCustom || (command.trim().length > 0 && approve)) &&
    !create.isPending;

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">New session</div>

        <label className="form-label">Agent</label>
        <select
          className="input"
          value={pluginId}
          onChange={(e) => setPluginId(e.target.value)}
        >
          {plugins?.map((p) => (
            <option key={p.id} value={p.id} disabled={!p.supported_on_this_platform}>
              {p.display_name}
              {p.id !== "custom_command" && !p.available ? " (not installed)" : ""}
            </option>
          ))}
        </select>
        {selected && selected.binary_path && (
          <div className="dim small mono">{selected.binary_path}</div>
        )}

        <label className="form-label">Working directory (absolute path on server)</label>
        <input
          className="input mono"
          placeholder="/home/you/project"
          value={cwd}
          onChange={(e) => setCwd(e.target.value)}
        />

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
              <input
                type="checkbox"
                checked={approve}
                onChange={(e) => setApprove(e.target.checked)}
              />
              I approve running this arbitrary command
            </label>
          </>
        )}

        {create.error && <div className="error">{String(create.error)}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={!canSubmit}
            onClick={() => create.mutate()}
          >
            {create.isPending ? "Starting…" : "Start"}
          </button>
        </div>
      </div>
    </div>
  );
}
