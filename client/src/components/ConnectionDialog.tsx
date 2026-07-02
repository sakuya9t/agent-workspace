import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { enrollDevice, probeHealth } from "../api";
import { useConnStore } from "../connectionStore";
import { useUiStore } from "../store";

/**
 * Connect the client to a daemon.
 *
 * - Local: the daemon serving this page (same-origin, no token).
 * - Remote: a direct LAN address, or an SSH-forwarded localhost port. Direct
 *   LAN daemons require an enrollment token; SSH-forwarded ports terminate on
 *   the remote's loopback and are trusted without one (leave the token blank).
 */
export function ConnectionDialog() {
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showConnection);
  const setShow = useUiStore((s) => s.setShowConnection);
  const profile = useConnStore();
  const setProfile = useConnStore((s) => s.setProfile);
  const reset = useConnStore((s) => s.reset);

  const [url, setUrl] = useState(profile.baseUrl);
  const [enrollToken, setEnrollToken] = useState("");
  const [deviceName, setDeviceName] = useState("desktop");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [ok, setOk] = useState<string | null>(null);

  if (!show) return null;

  const done = () => {
    qc.invalidateQueries();
    setShow(false);
  };

  const useLocal = () => {
    reset();
    setErr(null);
    setOk("Connected to local daemon.");
    done();
  };

  const connectRemote = async () => {
    const target = url.trim().replace(/\/$/, "");
    if (!target) {
      setErr("Enter a daemon URL, or use Local.");
      return;
    }
    setBusy(true);
    setErr(null);
    setOk(null);
    try {
      let token: string | null = null;
      if (enrollToken.trim()) {
        const res = await enrollDevice(target, enrollToken.trim(), deviceName || "device");
        token = res.device_token;
      }
      // Validate reachability + credentials before committing the profile.
      const health = await probeHealth(target, token);
      setProfile({
        baseUrl: target,
        token,
        serverId: null,
        label: `${health.hostname} (${new URL(target).host})`,
      });
      setOk(`Connected to ${health.hostname}.`);
      done();
    } catch (e) {
      setErr(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Connect to a daemon</div>

        <div className="conn-current dim small">
          Current:{" "}
          <b>{profile.baseUrl ? profile.label ?? profile.baseUrl : "local (same-origin)"}</b>
        </div>

        <label className="form-label">Local</label>
        <button className="btn" onClick={useLocal}>
          Use local daemon (this machine)
        </button>

        <div className="conn-divider">— or connect to a remote host —</div>

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
          placeholder="from the daemon log on first off-loopback start"
          value={enrollToken}
          onChange={(e) => setEnrollToken(e.target.value)}
        />

        <label className="form-label">Device name</label>
        <input
          className="input"
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
        />

        {err && <div className="error">{err}</div>}
        {ok && <div className="dim small">{ok}</div>}

        <div className="conn-hint dim small">
          Tip: for a private host, run{" "}
          <span className="mono">ssh -L 4600:127.0.0.1:4600 user@host</span> and connect to{" "}
          <span className="mono">http://localhost:4600</span> with no token.
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            Cancel
          </button>
          <button className="btn primary" disabled={busy} onClick={connectRemote}>
            {busy ? "Connecting…" : "Connect"}
          </button>
        </div>
      </div>
    </div>
  );
}
