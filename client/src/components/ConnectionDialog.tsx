import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, enrollDevice, probeHealth } from "../api";
import { localTarget, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";

/**
 * Manage the set of daemons the client talks to. The client aggregates all of
 * them in the left panel. Add a daemon by URL — with an enrollment token for a
 * direct LAN host, or blank for an SSH-forwarded localhost port (trusted as
 * loopback). The local daemon (this page's origin) is always present.
 */
export function ConnectionDialog() {
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showConnection);
  const setShow = useUiStore((s) => s.setShowConnection);
  const daemons = useConnStore((s) => s.daemons);
  const addDaemon = useConnStore((s) => s.addDaemon);
  const removeDaemon = useConnStore((s) => s.removeDaemon);

  const [url, setUrl] = useState("");
  const [enrollToken, setEnrollToken] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const { data: localEnrollToken } = useQuery({
    queryKey: ["enrollment-token"],
    queryFn: () => api.enrollmentToken(localTarget()),
    enabled: show,
    retry: false,
  });

  if (!show) return null;

  const addRemote = async () => {
    const targetUrl = url.trim().replace(/\/$/, "");
    if (!targetUrl) {
      setErr("Enter a daemon URL.");
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      let token: string | null = null;
      if (enrollToken.trim()) {
        const res = await enrollDevice(targetUrl, enrollToken.trim(), name || "desktop");
        token = res.device_token;
      }
      const health = await probeHealth(targetUrl, token);
      addDaemon({
        baseUrl: targetUrl,
        token,
        label: name.trim() || health.hostname || new URL(targetUrl).host,
      });
      qc.invalidateQueries({ queryKey: ["daemon"] });
      setUrl("");
      setEnrollToken("");
      setName("");
    } catch (e) {
      setErr(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Daemons</div>

        <div className="daemon-list">
          {daemons.map((d) => (
            <div key={d.id} className="daemon-row">
              <span className="tree-icon">⬢</span>
              <div className="daemon-meta">
                <div className="daemon-name">{d.label}</div>
                <div className="dim small mono">
                  {d.baseUrl || "same-origin (local)"}
                  {d.token ? " · 🔒 token" : ""}
                </div>
              </div>
              {d.id !== "local" && (
                <button
                  className="btn tiny"
                  onClick={() => {
                    removeDaemon(d.id);
                    qc.invalidateQueries({ queryKey: ["daemon"] });
                  }}
                >
                  remove
                </button>
              )}
            </div>
          ))}
        </div>

        {localEnrollToken && (
          <div className="dim small enroll-token">
            Enrollment token for other devices:{" "}
            <span className="mono selectable">{localEnrollToken}</span>
          </div>
        )}

        <div className="conn-divider">— add a remote daemon —</div>

        <label className="form-label">Daemon URL</label>
        <input
          className="input mono"
          placeholder="http://192.168.0.5:4600  or  http://localhost:4600 (SSH tunnel)"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />

        <label className="form-label">
          Enrollment token <span className="dim">(blank for SSH-tunnelled / loopback)</span>
        </label>
        <input
          className="input mono"
          placeholder="from `asm-daemon token` on that host"
          value={enrollToken}
          onChange={(e) => setEnrollToken(e.target.value)}
        />

        <label className="form-label">Label (optional)</label>
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} />

        {err && <div className="error">{err}</div>}

        <div className="conn-hint dim small">
          Tip: for a private host, run{" "}
          <span className="mono">ssh -L 4600:127.0.0.1:4600 user@host</span> and add{" "}
          <span className="mono">http://localhost:4600</span> with no token.
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            Close
          </button>
          <button className="btn primary" disabled={busy} onClick={addRemote}>
            {busy ? "Adding…" : "Add daemon"}
          </button>
        </div>
      </div>
    </div>
  );
}
