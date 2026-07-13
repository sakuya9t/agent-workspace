import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";
import { api, enrollDevice, listRelayNodes, probeHealth } from "../api";
import {
  daemonLabel,
  isLoopbackOrigin,
  localTarget,
  RelayConn,
  useConnStore,
} from "../connectionStore";
import { checkTargetUrl } from "../secureUrl";
import { useDaemonStates } from "../useDaemons";
import { useUiStore } from "../store";

type ConnectionMode = "existing" | "add";
type AddKind = "daemon" | "relay";

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
  const updateDaemon = useConnStore((s) => s.updateDaemon);
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
  const [addKind, setAddKind] = useState<AddKind>("daemon");

  const [localToken, setLocalToken] = useState("");
  const [localBusy, setLocalBusy] = useState(false);
  const [localErr, setLocalErr] = useState<string | null>(null);

  const local = daemons.find((d) => d.id === "local");
  // The daemon serves this bundle, so a phone on the LAN lands here same-origin
  // — but it is not a loopback peer, so it must enroll like any other device.
  // Without this the local row would sit at "unauthorized" with nothing to do.
  //
  // The page's own origin answers that for the common cases, but not for all of
  // them: a Vite dev server proxies /api server-side, so what the daemon judges
  // is the PROXY's address, not the browser's. A loopback-looking page whose
  // proxy dials the daemon over the LAN gets a 401 too. So take the 401 itself
  // as the other trigger — it is the ground truth, and it carries a status code
  // rather than a localized string.
  const localState = useDaemonStates().find((s) => s.daemon.id === "local");
  const localUnauthorized =
    (localState?.error as { status?: number } | undefined)?.status === 401;
  const localNeedsEnroll =
    Boolean(local) && !local?.token && (!isLoopbackOrigin() || localUnauthorized);

  // Enrolling this device is the one thing a phone MUST do here, and it lives on
  // the Existing tab — so open there rather than on "add another host". The need
  // can also appear late (the 401 lands after mount), hence the effect; it keys
  // on the transition, so it does not fight a user who then picks Add.
  const [mode, setMode] = useState<ConnectionMode>(localNeedsEnroll ? "existing" : "add");
  useEffect(() => {
    if (show && localNeedsEnroll) setMode("existing");
  }, [show, localNeedsEnroll]);

  // Live, so the warning appears as the URL is typed rather than on submit. A
  // plaintext URL is flagged, never blocked: a LAN daemon has no TLS to offer,
  // so refusing it would just make the product unusable on a trusted network.
  // Only an unparseable URL stops the add.
  const urlProblem = url.trim() ? checkTargetUrl(url.trim().replace(/\/$/, "")) : null;
  const relayUrlProblem = relayUrl.trim()
    ? checkTargetUrl(relayUrl.trim().replace(/\/$/, ""))
    : null;

  const { data: localEnrollToken } = useQuery({
    queryKey: ["enrollment-token"],
    queryFn: () => api.enrollmentToken(localTarget()),
    enabled: show,
    retry: false,
  });

  if (!show) return null;

  /**
   * Enroll this device against the daemon that served the page. Same-origin, so
   * there is no URL to type and no certificate to trust beyond the one the
   * browser already accepted to load this page — just the enrollment token.
   */
  const enrollLocal = async () => {
    const tok = localToken.trim();
    if (!tok) {
      setLocalErr(t("connection.errNoToken"));
      return;
    }
    setLocalBusy(true);
    setLocalErr(null);
    try {
      const res = await enrollDevice("", tok, name.trim() || navigator.platform || "device");
      await probeHealth("", res.device_token);
      updateDaemon("local", { token: res.device_token, connected: true });
      qc.invalidateQueries({ queryKey: ["daemon"] });
      qc.invalidateQueries({ queryKey: ["enrollment-token"] });
      setLocalToken("");
    } catch (e) {
      setLocalErr(String(e instanceof Error ? e.message : e));
    } finally {
      setLocalBusy(false);
    }
  };

  const addRemote = async () => {
    const targetUrl = url.trim().replace(/\/$/, "");
    if (!targetUrl) {
      setErr(t("connection.errNoUrl"));
      return;
    }
    if (urlProblem === "invalid") {
      setErr(t("connection.errBadUrl"));
      return;
    }
    if (urlProblem === "websocket") {
      setErr(t("connection.errWsScheme"));
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
      setMode("existing");
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
    if (relayUrlProblem === "invalid") {
      setRelayErr(t("relay.errBadUrl"));
      return;
    }
    if (relayUrlProblem === "websocket") {
      setRelayErr(t("relay.errWsScheme"));
      return;
    }
    const label = relayLabel.trim() || new URL(u).host;
    setRelayErr(null);
    addRelay({ url: u, accessKey: relayKey.trim(), label });
    setRelayUrl("");
    setRelayKey("");
    setRelayLabel("");
    setMode("existing");
  };

  return (
    <div className="modal-backdrop" onClick={() => setShow(false)}>
      <div className="modal conn-modal" onClick={(e) => e.stopPropagation()}>
        <div className="conn-modal-head">
          <div>
            <div className="modal-title">{t("connection.title")}</div>
            <div className="dim small">{t("connection.subtitle")}</div>
          </div>
        </div>

        <div className="conn-primary-tabs" role="tablist" aria-label={t("connection.operationTabs")}>
          <button
            type="button"
            role="tab"
            className={"conn-primary-tab" + (mode === "existing" ? " on" : "")}
            aria-selected={mode === "existing"}
            onClick={() => setMode("existing")}
          >
            <span className="conn-primary-tab-title">{t("connection.existingTab")}</span>
            <span className="conn-primary-tab-desc">{t("connection.existingTabDesc")}</span>
          </button>
          <button
            type="button"
            role="tab"
            className={"conn-primary-tab" + (mode === "add" ? " on" : "")}
            aria-selected={mode === "add"}
            onClick={() => setMode("add")}
          >
            <span className="conn-primary-tab-title">{t("connection.addTab")}</span>
            <span className="conn-primary-tab-desc">{t("connection.addTabDesc")}</span>
          </button>
        </div>

        {mode === "existing" ? (
          <div className="conn-pane">
            <section className="conn-section">
              <div className="conn-section-head">
                <div>
                  <div className="conn-section-title">{t("connection.currentDaemonsTitle")}</div>
                  <div className="dim small">{t("connection.currentDaemonsHint")}</div>
                </div>
                <span className="conn-count">{t("connection.daemonCount", { count: daemons.length })}</span>
              </div>

              <div className="daemon-list conn-daemon-list">
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

              {localNeedsEnroll && (
                <div className="conn-callout">
                  <div className="conn-callout-title">{t("connection.enrollLocalTitle")}</div>
                  <div className="dim small">
                    {t("connection.enrollLocalHint", { cmd: "asm-daemon token" })}
                  </div>
                  <div className="conn-node-connect">
                    <input
                      className="input mono small"
                      placeholder={t("connection.tokenPlaceholder", { cmd: "asm-daemon token" })}
                      value={localToken}
                      autoCapitalize="none"
                      autoCorrect="off"
                      spellCheck={false}
                      onChange={(e) => setLocalToken(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !localBusy) enrollLocal();
                      }}
                    />
                    <button className="btn tiny" disabled={localBusy} onClick={enrollLocal}>
                      {localBusy ? t("connection.enrolling") : t("connection.enroll")}
                    </button>
                  </div>
                  {localErr && <div className="conn-node-error error-line small">{localErr}</div>}
                </div>
              )}

              {localEnrollToken && (
                <div className="conn-callout">
                  <div className="conn-callout-title">{t("connection.enrollTokenLabel")}</div>
                  <span className="mono selectable">{localEnrollToken}</span>
                </div>
              )}
            </section>

            <section className="conn-section">
              <div className="conn-section-head">
                <div>
                  <div className="conn-section-title">{t("connection.savedRelaysTitle")}</div>
                  <div className="dim small">{t("connection.savedRelaysHint")}</div>
                </div>
                <span className="conn-count">{t("connection.relayCount", { count: relays.length })}</span>
              </div>

              {relays.length === 0 ? (
                <div className="conn-empty">{t("relay.empty")}</div>
              ) : (
                relays.map((r) => (
                  <RelayRow key={r.id} relay={r} onRemove={() => removeRelay(r.id)} />
                ))
              )}
            </section>
          </div>
        ) : (
          <div className="conn-pane">
            <div className="conn-add-shell">
              <div className="conn-add-step">{t("connection.addChoiceLabel")}</div>
              <div className="conn-type-tabs" role="tablist" aria-label={t("connection.typeTabs")}>
                <button
                  type="button"
                  role="tab"
                  className={"conn-type-tab" + (addKind === "daemon" ? " on" : "")}
                  aria-selected={addKind === "daemon"}
                  onClick={() => setAddKind("daemon")}
                >
                  <span className="conn-type-tab-title">{t("connection.directType")}</span>
                  <span className="conn-type-tab-desc">{t("connection.directTypeDesc")}</span>
                </button>
                <button
                  type="button"
                  role="tab"
                  className={"conn-type-tab" + (addKind === "relay" ? " on" : "")}
                  aria-selected={addKind === "relay"}
                  onClick={() => setAddKind("relay")}
                >
                  <span className="conn-type-tab-title">{t("connection.relayType")}</span>
                  <span className="conn-type-tab-desc">{t("connection.relayTypeDesc")}</span>
                </button>
              </div>

              {addKind === "relay" ? (
                <div className="conn-form">
                  <div className="conn-form-title">{t("connection.addRelayTitle")}</div>
                  <div className="dim small">{t("connection.addRelayHint")}</div>

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
                  {relayUrlProblem === "insecure" && (
                    <div className="conn-insecure-warn small">{t("relay.insecureWarn")}</div>
                  )}
                  {relayErr && <div className="error conn-form-error">{relayErr}</div>}
                  <div className="conn-hint dim small">{t("relay.tip")}</div>
                  <div className="conn-form-actions">
                    <button className="btn primary" onClick={addRelayConn}>
                      {t("relay.add")}
                    </button>
                  </div>
                </div>
              ) : (
                <div className="conn-form">
                  <div className="conn-form-title">{t("connection.addDirectTitle")}</div>
                  <div className="dim small">{t("connection.addDirectHint")}</div>

                  <label className="form-label">{t("connection.urlLabel")}</label>
                  <input
                    className="input mono"
                    placeholder={t("connection.urlPlaceholder", {
                      lan: "https://asm.example.com",
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

                  {urlProblem === "insecure" && (
                    <div className="conn-insecure-warn small">{t("connection.insecureWarn")}</div>
                  )}

                  {err && <div className="error conn-form-error">{err}</div>}

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

                  <div className="conn-form-actions">
                    <button className="btn primary" disabled={busy} onClick={addRemote}>
                      {busy ? t("connection.adding") : t("connection.add")}
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        <div className="modal-actions conn-close-actions">
          <button className="btn" onClick={() => setShow(false)}>
            {t("connection.close")}
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
    <div className="conn-relay-card">
      <div className="conn-relay-head">
        <span className="tree-icon">⬡</span>
        <div className="daemon-meta">
          <div className="daemon-name">{relay.label}</div>
          <div className="dim small mono">{relay.url}</div>
        </div>
        <button className="btn tiny" onClick={onRemove}>
          {t("relay.remove")}
        </button>
      </div>

      <div className="conn-relay-nodes">
        <div className="conn-nested-label">{t("relay.nodesTitle")}</div>
        {error ? (
          <div className="dim small">{t("relay.unreachable")}</div>
        ) : !nodes ? (
          <div className="dim small">{t("relay.discovering")}</div>
        ) : nodes.length === 0 ? (
          <div className="dim small">{t("relay.noNodes")}</div>
        ) : (
          nodes.map((n) => (
            <div key={n.node_id} className="conn-node-row">
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
                <div className="conn-node-connect">
                  <input
                    className="input mono small"
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
        {nodeErr && <div className="error small conn-node-error">{nodeErr}</div>}
      </div>
    </div>
  );
}
