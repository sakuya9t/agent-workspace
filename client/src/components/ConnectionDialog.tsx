import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";
import { api, enrollDevice, listRelayNodes, probeHealth } from "../api";
import { daemonLabel, localTarget, RelayConn, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";

/**
 * Manage the daemons and relays the client talks to. The client aggregates all
 * daemons in the left panel. A daemon is added by URL (with an enrollment token
 * for a direct LAN host, or blank for an SSH-forwarded loopback port), or
 * discovered through a relay — which needs no client-side tunnels and so works
 * on any device. The local daemon (this page's origin) is always present.
 */
export function ConnectionDialog() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const show = useUiStore((s) => s.showConnection);
  const setShow = useUiStore((s) => s.setShowConnection);
  const daemons = useConnStore((s) => s.daemons);
  const addDaemon = useConnStore((s) => s.addDaemon);
  const removeDaemon = useConnStore((s) => s.removeDaemon);
  const relays = useConnStore((s) => s.relays);
  const addRelay = useConnStore((s) => s.addRelay);
  const removeRelay = useConnStore((s) => s.removeRelay);

  const [url, setUrl] = useState("");
  const [enrollToken, setEnrollToken] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const [relayUrl, setRelayUrl] = useState("");
  const [relayKey, setRelayKey] = useState("");
  const [relayLabel, setRelayLabel] = useState("");
  const [relayErr, setRelayErr] = useState<string | null>(null);

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

  const addRelayConn = () => {
    const u = relayUrl.trim().replace(/\/$/, "");
    if (!u) {
      setRelayErr(t("relay.errNoUrl"));
      return;
    }
    if (!relayKey.trim()) {
      setRelayErr(t("relay.errNoKey"));
      return;
    }
    setRelayErr(null);
    addRelay({ url: u, accessKey: relayKey.trim(), label: relayLabel.trim() || new URL(u).host });
    setRelayUrl("");
    setRelayKey("");
    setRelayLabel("");
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

        {/* Relays: reach private hosts that dial out to the relay, no tunnels. */}
        <div className="conn-divider">{t("relay.sectionTitle")}</div>
        {relays.length === 0 ? (
          <div className="conn-hint dim small">{t("relay.empty")}</div>
        ) : (
          relays.map((r) => (
            <RelayRow key={r.id} relay={r} onRemove={() => removeRelay(r.id)} />
          ))
        )}

        <div className="conn-divider">{t("relay.addDivider")}</div>
        <label className="form-label">{t("relay.urlLabel")}</label>
        <input
          className="input mono"
          placeholder={t("relay.urlPlaceholder")}
          value={relayUrl}
          onChange={(e) => setRelayUrl(e.target.value)}
        />
        <label className="form-label">{t("relay.keyLabel")}</label>
        <input
          className="input mono"
          placeholder={t("relay.keyPlaceholder")}
          value={relayKey}
          onChange={(e) => setRelayKey(e.target.value)}
        />
        <label className="form-label">{t("relay.labelLabel")}</label>
        <input className="input" value={relayLabel} onChange={(e) => setRelayLabel(e.target.value)} />
        {relayErr && <div className="error">{relayErr}</div>}
        <div className="conn-hint dim small">{t("relay.tip")}</div>
        <div className="modal-actions">
          <button className="btn" onClick={addRelayConn}>
            {t("relay.add")}
          </button>
        </div>

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

/**
 * One added relay: its label/URL, a remove control, and the nodes it has
 * discovered. Each not-yet-connected online node offers an enrollment-token
 * field + Connect, which enrolls a device THROUGH the relay and stores the node
 * as an ordinary daemon (reached via `/n/<node_id>`).
 */
function RelayRow({ relay, onRemove }: { relay: RelayConn; onRemove: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const daemons = useConnStore((s) => s.daemons);
  const addDaemon = useConnStore((s) => s.addDaemon);

  const [tokens, setTokens] = useState<Record<string, string>>({});
  const [connecting, setConnecting] = useState<string | null>(null);
  const [nodeErr, setNodeErr] = useState<string | null>(null);

  const { data: nodes, error } = useQuery({
    queryKey: ["relay-nodes", relay.id, relay.url, relay.accessKey],
    queryFn: () => listRelayNodes(relay.url, relay.accessKey),
    refetchInterval: 3000,
    retry: false,
  });

  const nodeBaseUrl = (nodeId: string) => `${relay.url.replace(/\/$/, "")}/n/${nodeId}`;
  // Attribute a downstream to its gateway by label (falling back to the id).
  const gatewayLabel = (id: string) =>
    nodes?.find((x) => x.node_id === id)?.label || id.slice(0, 8);
  const isConnected = (nodeId: string) =>
    daemons.some((d) => d.via === relay.id && d.baseUrl === nodeBaseUrl(nodeId));

  const connect = async (nodeId: string, label: string) => {
    const tok = (tokens[nodeId] ?? "").trim();
    if (!tok) {
      setNodeErr(t("connection.tokenLabel"));
      return;
    }
    setConnecting(nodeId);
    setNodeErr(null);
    try {
      const baseUrl = nodeBaseUrl(nodeId);
      const res = await enrollDevice(baseUrl, tok, "desktop", relay.accessKey);
      await probeHealth(baseUrl, res.device_token, relay.accessKey);
      addDaemon({
        baseUrl,
        token: res.device_token,
        relayKey: relay.accessKey,
        via: relay.id,
        label: label || nodeId.slice(0, 8),
      });
      qc.invalidateQueries({ queryKey: ["daemon"] });
      setTokens((prev) => ({ ...prev, [nodeId]: "" }));
    } catch (e) {
      setNodeErr(String(e instanceof Error ? e.message : e));
    } finally {
      setConnecting(null);
    }
  };

  return (
    <div className="daemon-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span className="tree-icon">⬡</span>
        <div className="daemon-meta" style={{ flex: 1 }}>
          <div className="daemon-name">{relay.label}</div>
          <div className="dim small mono">{relay.url}</div>
        </div>
        <button className="btn tiny" onClick={onRemove}>
          {t("relay.remove")}
        </button>
      </div>

      {error ? (
        <div className="dim small" style={{ paddingLeft: 24 }}>
          {t("relay.unreachable")}
        </div>
      ) : !nodes ? (
        <div className="dim small" style={{ paddingLeft: 24 }}>
          {t("relay.discovering")}
        </div>
      ) : nodes.length === 0 ? (
        <div className="dim small" style={{ paddingLeft: 24 }}>
          {t("relay.noNodes")}
        </div>
      ) : (
        nodes.map((n) => (
          <div key={n.node_id} style={{ paddingLeft: 24, marginTop: 6 }}>
            <div className="small">
              <span className="mono">{n.label || n.node_id.slice(0, 8)}</span>
              {" · "}
              <span className={n.online ? "ok" : "dim"}>
                {n.online ? t("relay.online") : t("relay.offline")}
              </span>
              {n.via && <span className="dim">{" · "}{t("relay.viaGateway", { gateway: gatewayLabel(n.via) })}</span>}
            </div>
            {isConnected(n.node_id) ? (
              <div className="dim small">{t("relay.connected")}</div>
            ) : (
              <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
                <input
                  className="input mono small"
                  style={{ flex: 1 }}
                  placeholder={t("relay.nodeTokenPlaceholder")}
                  value={tokens[n.node_id] ?? ""}
                  onChange={(e) => setTokens((p) => ({ ...p, [n.node_id]: e.target.value }))}
                  disabled={!n.online}
                />
                <button
                  className="btn tiny"
                  disabled={!n.online || connecting === n.node_id}
                  onClick={() => connect(n.node_id, n.label)}
                >
                  {connecting === n.node_id ? t("relay.connecting") : t("relay.connect")}
                </button>
              </div>
            )}
          </div>
        ))
      )}
      {nodeErr && <div className="error small">{nodeErr}</div>}
    </div>
  );
}
