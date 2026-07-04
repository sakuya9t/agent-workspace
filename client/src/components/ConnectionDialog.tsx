import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";
import { api, enrollDevice, probeHealth } from "../api";
import { daemonLabel, localTarget, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";

/**
 * Manage the set of daemons the client talks to. The client aggregates all of
 * them in the left panel. Add a daemon by URL — with an enrollment token for a
 * direct LAN host, or blank for an SSH-forwarded localhost port (trusted as
 * loopback). The local daemon (this page's origin) is always present.
 */
export function ConnectionDialog() {
  const { t } = useTranslation();
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
      setErr(t("connection.errNoUrl"));
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
        <div className="modal-title">{t("connection.title")}</div>

        <div className="daemon-list">
          {daemons.map((d) => (
            <div key={d.id} className="daemon-row">
              <span className="tree-icon">⬢</span>
              <div className="daemon-meta">
                <div className="daemon-name">{daemonLabel(d)}</div>
                <div className="dim small mono">
                  {d.baseUrl || t("connection.sameOrigin")}
                  {d.token && <>{" · "}{t("connection.tokenTag")}</>}
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
                  {t("connection.remove")}
                </button>
              )}
            </div>
          ))}
        </div>

        {localEnrollToken && (
          <div className="dim small enroll-token">
            {t("connection.enrollTokenLabel")}{" "}
            <span className="mono selectable">{localEnrollToken}</span>
          </div>
        )}

        <div className="conn-divider">{t("connection.addDivider")}</div>

        <label className="form-label">{t("connection.urlLabel")}</label>
        <input
          className="input mono"
          placeholder={t("connection.urlPlaceholder", {
            lan: "http://192.168.0.5:4600",
            tunnel: "http://localhost:4600",
          })}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />

        <label className="form-label">
          {t("connection.tokenLabel")}{" "}
          <span className="dim">{t("connection.tokenHint")}</span>
        </label>
        <input
          className="input mono"
          placeholder={t("connection.tokenPlaceholder", { cmd: "asm-daemon token" })}
          value={enrollToken}
          onChange={(e) => setEnrollToken(e.target.value)}
        />

        <label className="form-label">{t("connection.nameLabel")}</label>
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} />

        {err && <div className="error">{err}</div>}

        <div className="conn-hint dim small">
          <Trans
            i18nKey="connection.sshTip"
            components={{ cmd: <span className="mono" /> }}
            values={{
              ssh: "ssh -L 4600:127.0.0.1:4600 user@host",
              url: "http://localhost:4600",
            }}
          />
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={() => setShow(false)}>
            {t("connection.close")}
          </button>
          <button className="btn primary" disabled={busy} onClick={addRemote}>
            {busy ? t("connection.adding") : t("connection.add")}
          </button>
        </div>
      </div>
    </div>
  );
}
