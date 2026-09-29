import { type ReactNode, useMemo, useState } from "react";
import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  api,
  ConfigUpdate,
  ConfigView,
  getUiServerConfig,
  putUiServerConfig,
} from "../api";
import { DaemonConn, daemonLabel, targetOf, useConnStore } from "../connectionStore";
import { useUiStore } from "../store";
import {
  ComponentKey,
  componentOn,
  CONFIG_DEFAULTS,
  CONFIG_KEYS,
  ConfigKey,
  ConfigValues,
  isSecretKey,
  SECRET_KEYS,
  validateConfig,
} from "../startupConfig";

/**
 * Startup settings for the hosts this client can reach: the UI server that
 * serves this page (Vite's `/__asm/config`) and every connected daemon
 * (`/api/config`). Each host keeps its own `startup.conf`, which
 * `scripts/start.sh` starts from; saving here changes what the NEXT start does
 * on that host — nothing restarts from the browser. A host serving this page
 * that also runs the daemon it proxies to shares one file, so it is listed once.
 */
export function ConfigDialog() {
  const show = useUiStore((s) => s.showConfig);
  return show ? <ConfigDialogBody /> : null;
}

/** The independent components a host can run: switch, title, description. */
const COMPONENTS = [
  ["ASM_RUN_DAEMON", "config.field.ASM_RUN_DAEMON", "config.compDaemonDesc"],
  ["ASM_RUN_RELAY", "config.field.ASM_RUN_RELAY", "config.compRelayDesc"],
  ["ASM_RUN_UI", "config.field.ASM_RUN_UI", "config.compUiDesc"],
] as const;

interface HostEntry {
  key: string;
  kind: "ui" | "daemon" | "both";
  daemon?: DaemonConn;
  view?: ConfigView;
  error?: unknown;
  loading: boolean;
  queryKey: readonly unknown[];
  save: (body: ConfigUpdate) => Promise<ConfigView>;
}

/** Edits in progress for one host. Secrets are replaced or cleared, never shown. */
interface Draft {
  values: Record<ConfigKey, string>;
  secrets: Partial<Record<ConfigKey, string>>;
  cleared: ConfigKey[];
}

const UI_QUERY_KEY = ["startup-config", "ui"] as const;
const daemonQueryKey = (d: DaemonConn) =>
  ["startup-config", d.id, d.baseUrl, d.token, d.relayKey ?? null] as const;

/**
 * Effective values: what's saved, else what the scripts default to — an empty
 * saved value means the default too, so it reads as one. A field the user
 * clears is saved empty, i.e. back to the default its placeholder shows.
 */
function initialValues(view: ConfigView): Record<ConfigKey, string> {
  const out = { ...CONFIG_DEFAULTS };
  for (const key of CONFIG_KEYS) {
    const v = view.values[key];
    if (isSecretKey(key)) out[key] = "";
    else if (v) out[key] = v;
  }
  return out;
}

const blankDraft = (view: ConfigView): Draft => ({
  values: initialValues(view),
  secrets: {},
  cleared: [],
});

function changes(view: ConfigView, draft: Draft): ConfigValues {
  const base = initialValues(view);
  const out: ConfigValues = {};
  for (const key of CONFIG_KEYS) {
    if (isSecretKey(key)) {
      if (draft.secrets[key]) out[key] = draft.secrets[key];
      else if (draft.cleared.includes(key)) out[key] = "";
    } else if (draft.values[key] !== base[key]) {
      out[key] = draft.values[key];
    }
  }
  return out;
}

function isLoopbackHost(host: string): boolean {
  const h = host.replace(/^\[(.*)\]$/, "$1").toLowerCase();
  return h === "localhost" || h === "::1" || h.startsWith("127.");
}

function bindHost(bind: string): string {
  const i = bind.lastIndexOf(":");
  return i > 0 ? bind.slice(0, i) : bind;
}

function ConfigDialogBody() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const setShow = useUiStore((s) => s.setShowConfig);
  const daemons = useConnStore((s) => s.daemons);
  const relays = useConnStore((s) => s.relays);
  const connected = useMemo(() => daemons.filter((d) => d.connected), [daemons]);

  const uiQ = useQuery({ queryKey: UI_QUERY_KEY, queryFn: getUiServerConfig, retry: false, staleTime: 0 });
  const daemonQs = useQueries({
    queries: connected.map((d) => ({
      queryKey: daemonQueryKey(d),
      queryFn: () => api.getConfig(targetOf(d)),
      retry: false,
      staleTime: 0,
    })),
  });

  const hosts: HostEntry[] = useMemo(() => {
    const out: HostEntry[] = [];
    const ui = uiQ.data ?? null;
    // The daemon whose config file IS the UI server's (same host, same file).
    const twin = ui
      ? connected.findIndex((_, i) => {
          const v = daemonQs[i]?.data;
          return v && v.hostname === ui.hostname && v.path === ui.path;
        })
      : -1;
    if (ui) {
      out.push({
        key: "ui",
        kind: twin >= 0 ? "both" : "ui",
        daemon: twin >= 0 ? connected[twin] : undefined,
        view: ui,
        loading: false,
        queryKey: UI_QUERY_KEY,
        save: putUiServerConfig,
      });
    }
    connected.forEach((d, i) => {
      if (i === twin) return;
      const q = daemonQs[i];
      out.push({
        key: "d:" + d.id,
        kind: "daemon",
        daemon: d,
        view: q?.data,
        error: q?.error ?? undefined,
        loading: !!q?.isLoading,
        queryKey: daemonQueryKey(d),
        save: (body) => api.putConfig(targetOf(d), body),
      });
    });
    return out;
  }, [uiQ.data, connected, daemonQs]);

  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const selected = hosts.find((h) => h.key === selectedKey) ?? hosts[0];
  /** What the host is and how it is reached — without repeating its name. */
  const hostSub = (h: HostEntry) => {
    const relay = h.daemon?.via ? relays.find((r) => r.id === h.daemon?.via) : undefined;
    const parts = [
      h.kind === "ui"
        ? t("config.kindUi")
        : h.kind === "both"
          ? t("config.kindBoth")
          : relay
            ? t("config.kindRelayedNamed", { relay: relay.label })
            : h.daemon?.via
              ? t("config.kindRelayed")
              : t("config.kindDaemon"),
    ];
    const host = h.view?.hostname;
    if (host && !hostName(h).includes(host)) parts.push(host);
    return parts.join(" · ");
  };
  const hostName = (h: HostEntry) =>
    h.daemon ? daemonLabel(h.daemon) : t("config.uiServerNamed", { host: h.view?.hostname ?? "" });
  const isDirty = (h: HostEntry) =>
    !!h.view && !!drafts[h.key] && Object.keys(changes(h.view, drafts[h.key])).length > 0;

  const close = () => {
    if (hosts.some(isDirty) && !window.confirm(t("config.discardConfirm"))) return;
    setShow(false);
  };

  const pick = (key: string) => {
    setSelectedKey(key);
    setErr(null);
    setConflict(false);
    setNotice(null);
  };

  const view = selected?.view;
  const draft = selected && view ? (drafts[selected.key] ?? blankDraft(view)) : null;
  // Functional updates: several edits between renders must all land.
  const mutateDraft = (fn: (d: Draft) => Draft) => {
    if (!selected || !view) return;
    const key = selected.key;
    const base = view;
    setDrafts((prev) => ({ ...prev, [key]: fn(prev[key] ?? blankDraft(base)) }));
    setNotice(null);
  };
  const update = (patch: Partial<Record<ConfigKey, string>>) =>
    mutateDraft((d) => ({ ...d, values: { ...d.values, ...patch } }));
  /** `null` clears a saved secret; a string replaces it; `""` keeps it. */
  const setSecret = (key: ConfigKey, value: string | null) =>
    mutateDraft((d) => {
      const secrets = { ...d.secrets };
      delete secrets[key];
      if (value) secrets[key] = value;
      const cleared = d.cleared.filter((k) => k !== key);
      return { ...d, secrets, cleared: value === null ? [...cleared, key] : cleared };
    });
  const errors = useMemo(() => {
    if (!draft) return [];
    const vals: ConfigValues = { ...draft.values };
    for (const key of SECRET_KEYS) vals[key] = draft.secrets[key] ?? "";
    return validateConfig(vals, (k) =>
      draft.secrets[k] ? true : draft.cleared.includes(k) ? false : !!view?.secrets[k],
    );
  }, [draft, view]);
  const errorFor = (key: ConfigKey) => {
    const e = errors.find((x) => x.key === key);
    return e ? t(`config.err.${e.code}`) : null;
  };

  const save = async () => {
    if (!selected || !view || !draft) return;
    setBusy(true);
    setErr(null);
    setConflict(false);
    setNotice(null);
    try {
      const next = await selected.save({ revision: view.revision, values: changes(view, draft) });
      qc.setQueryData(selected.queryKey, next);
      // A merged entry's daemon twin reads the same file; refresh it too.
      qc.invalidateQueries({ queryKey: ["startup-config"] });
      setDrafts((prev) => {
        const rest = { ...prev };
        delete rest[selected.key];
        return rest;
      });
      setNotice(t("config.saved", { host: hostName(selected) }));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setConflict((e as { status?: number }).status === 409);
    } finally {
      setBusy(false);
    }
  };

  const reload = () => {
    if (!selected) return;
    setDrafts((prev) => {
      const rest = { ...prev };
      delete rest[selected.key];
      return rest;
    });
    setErr(null);
    setConflict(false);
    qc.invalidateQueries({ queryKey: selected.queryKey });
  };

  const stillLoading = uiQ.isLoading || daemonQs.some((q) => q.isLoading);
  const dirty = selected ? isDirty(selected) : false;

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal conn-modal config-modal" onClick={(e) => e.stopPropagation()}>
        <div className="conn-modal-head">
          <div>
            <div className="modal-title">{t("config.title")}</div>
            <div className="dim small">{t("config.subtitle")}</div>
          </div>
        </div>

        {hosts.length === 0 ? (
          <div className="conn-empty">{stillLoading ? t("config.loading") : t("config.noHosts")}</div>
        ) : (
          <>
            <div className="conn-add-step">{t("config.hostsLabel")}</div>
            <div
              className="config-hosts"
              role="tablist"
              aria-orientation="vertical"
              aria-label={t("config.hostTabs")}
            >
              {hosts.map((h) => (
                <button
                  key={h.key}
                  type="button"
                  role="tab"
                  aria-selected={h === selected}
                  className={"config-host" + (h === selected ? " on" : "")}
                  onClick={() => pick(h.key)}
                >
                  <span className="tree-icon">⬢</span>
                  <span className="config-host-meta">
                    <span className="config-host-name">{hostName(h)}</span>
                    <span className="config-host-sub">{hostSub(h)}</span>
                  </span>
                  <span className="config-host-tags">
                    {isDirty(h) && <span className="config-tag warn-tag">{t("config.unsaved")}</span>}
                    {!!h.view?.pending?.length && <span className="config-tag">{t("config.pendingTag")}</span>}
                    {!h.view && !h.loading && <span className="config-tag">{t("config.unavailableTag")}</span>}
                  </span>
                </button>
              ))}
            </div>

            {selected && (
              <div className="config-body">
                {!view ? (
                  selected.loading ? (
                    <div className="conn-empty">{t("config.loading")}</div>
                  ) : (
                    <div className="conn-empty">
                      {(selected.error as { status?: number } | undefined)?.status === 404
                        ? t("config.unsupported")
                        : selected.error
                          ? String((selected.error as Error).message ?? selected.error)
                          : t("config.offline")}
                    </div>
                  )
                ) : (
                  draft && (
                    <ConfigForm
                      entry={selected}
                      view={view}
                      draft={draft}
                      update={update}
                      setSecret={setSecret}
                      errorFor={errorFor}
                    />
                  )
                )}
              </div>
            )}
          </>
        )}

        {err && (
          <div className="error conn-form-error">
            {conflict ? t("config.conflict") : err}{" "}
            {conflict && (
              <button className="btn tiny" onClick={reload}>
                {t("config.reload")}
              </button>
            )}
          </div>
        )}
        {notice && <div className="config-notice">{notice}</div>}

        <div className="modal-actions conn-close-actions">
          <button className="btn" onClick={close}>
            {t("config.close")}
          </button>
          {view && (
            <button
              className="btn primary"
              disabled={busy || !dirty || errors.length > 0}
              onClick={save}
            >
              {busy ? t("config.saving") : t("config.save")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

interface FormProps {
  entry: HostEntry;
  view: ConfigView;
  draft: Draft;
  update: (patch: Partial<Record<ConfigKey, string>>) => void;
  setSecret: (key: ConfigKey, value: string | null) => void;
  errorFor: (key: ConfigKey) => string | null;
}

/** Settings edited through a labelled field (the component switches are cards). */
type FieldKey = Exclude<ConfigKey, ComponentKey>;

function ConfigForm({ entry, view, draft, update, setSecret, errorFor }: FormProps) {
  const { t } = useTranslation();
  const v = draft.values;
  const daemon = componentOn(v, "ASM_RUN_DAEMON");
  const relay = componentOn(v, "ASM_RUN_RELAY");
  const ui = componentOn(v, "ASM_RUN_UI");

  const text = (key: FieldKey, placeholder?: string) => (
    <Field key={key} k={key} error={errorFor(key)}>
      <input
        className="input mono"
        value={v[key]}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => update({ [key]: e.target.value })}
      />
    </Field>
  );

  // `unset` names what an empty secret means, when that is not simply "none".
  const secret = (key: FieldKey, unset: string = t("config.secretUnset")) => {
    const cleared = draft.cleared.includes(key);
    const saved = !!view.secrets[key];
    return (
      <Field key={key} k={key} error={errorFor(key)}>
        <div className="config-secret">
          <input
            className="input mono"
            value={draft.secrets[key] ?? ""}
            placeholder={cleared ? t("config.secretCleared") : saved ? t("config.secretSet") : unset}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setSecret(key, e.target.value)}
          />
          {saved && !cleared && !draft.secrets[key] && (
            <button type="button" className="btn tiny" onClick={() => setSecret(key, null)}>
              {t("config.secretClear")}
            </button>
          )}
          {cleared && (
            <button type="button" className="btn tiny" onClick={() => setSecret(key, "")}>
              {t("config.secretUndo")}
            </button>
          )}
        </div>
      </Field>
    );
  };

  // Consequences worth a second look before saving.
  const warnings: string[] = [];
  const servesPage = entry.kind !== "daemon";
  if (entry.daemon && !daemon) warnings.push(t("config.warnNoDaemon"));
  if (
    entry.kind === "daemon" &&
    entry.daemon?.baseUrl &&
    !entry.daemon.via &&
    daemon &&
    isLoopbackHost(bindHost(v.ASM_BIND))
  ) {
    warnings.push(t("config.warnLoopbackRemote", { url: entry.daemon.baseUrl }));
  }
  if (servesPage && !ui) warnings.push(t("config.warnUiStops"));
  if (servesPage && ui && isLoopbackHost(v.ASM_UI_HOST) && !isLoopbackHost(window.location.hostname)) {
    warnings.push(t("config.warnUiLoopback", { host: window.location.host }));
  }

  const pending = view.pending ?? [];

  return (
    <div className="config-form">
      <div className="config-meta dim small">
        {t("config.fileLabel")} <span className="mono selectable-plain">{view.path}</span>
      </div>
      {!view.exists && <div className="conn-callout">{t("config.noFile")}</div>}
      {view.source === "ui" && view.managed === false && (
        <div className="conn-callout">{t("config.notManaged")}</div>
      )}
      {pending.length > 0 && (
        <div className="conn-callout config-pending">
          {t("config.pending", {
            count: pending.length,
            keys: pending.map((k) => t(`config.field.${k}`)).join(", "),
          })}
        </div>
      )}

      <div className="conn-add-step config-step">{t("config.componentsLabel")}</div>
      <div className="conn-type-tabs config-components" role="group" aria-label={t("config.componentsLabel")}>
        {COMPONENTS.map(([key, title, desc]) => {
          const on = componentOn(v, key);
          return (
            <button
              key={key}
              type="button"
              role="switch"
              aria-checked={on}
              className={"conn-type-tab config-component" + (on ? " on" : "")}
              onClick={() => update({ [key]: on ? "0" : "1" })}
            >
              <span className="conn-type-tab-title">
                {t(title)}
                <span className={"config-state" + (on ? " on" : "")}>{on ? t("config.on") : t("config.off")}</span>
              </span>
              <span className="conn-type-tab-desc">{t(desc)}</span>
              <span className="config-key">{key}</span>
            </button>
          );
        })}
      </div>
      {errorFor("ASM_RUN_DAEMON") && <div className="config-field-error">{errorFor("ASM_RUN_DAEMON")}</div>}

      {daemon && (
        <section className="config-section">
          <div className="conn-section-title">{t("config.sectionDaemon")}</div>
          <div className="config-grid">
            {text("ASM_BIND", CONFIG_DEFAULTS.ASM_BIND)}
            {text("ASM_NODE_LABEL", t("config.labelPlaceholder", { host: view.hostname }))}
          </div>
          {secret("ASM_ENROLLMENT_TOKEN", t("config.enrollmentGenerated"))}
          <div className="config-grid">
            {text("ASM_RELAY_URL", t("config.ph.relayUrl"))}
            {secret("ASM_RELAY_KEY")}
          </div>
          {text("ASM_RELAY_DOWNSTREAMS", t("config.ph.downstreams"))}
        </section>
      )}

      {relay && (
        <section className="config-section">
          <div className="conn-section-title">{t("config.sectionRelay")}</div>
          <div className="config-grid">
            {text("ASM_RELAY_BIND", CONFIG_DEFAULTS.ASM_RELAY_BIND)}
            {secret("ASM_RELAY_KEYS")}
          </div>
        </section>
      )}

      {ui && (
        <section className="config-section">
          <div className="conn-section-title">{t("config.sectionUi")}</div>
          <div className="config-grid">
            {text("ASM_UI_HOST", CONFIG_DEFAULTS.ASM_UI_HOST)}
            {text("ASM_UI_PORT", CONFIG_DEFAULTS.ASM_UI_PORT)}
          </div>
          {text("ASM_UI_ALLOWED_HOSTS", t("config.ph.allowedHosts"))}
          {!daemon && (
            <>
              <div className="dim small config-subhint">{t("config.uiWithoutDaemon")}</div>
              <div className="config-grid">
                {text("ASM_UI_DAEMON", t("config.ph.uiDaemon"))}
                {secret("ASM_UI_DAEMON_TOKEN")}
              </div>
            </>
          )}
        </section>
      )}

      {(daemon || relay) && (
        <section className="config-section">
          <div className="conn-section-title">{t("config.sectionBuild")}</div>
          <Field k="ASM_PROFILE" error={errorFor("ASM_PROFILE")}>
            <select
              className="input"
              value={v.ASM_PROFILE}
              onChange={(e) => update({ ASM_PROFILE: e.target.value })}
            >
              <option value="debug">{t("config.profileDebug")}</option>
              <option value="release">{t("config.profileRelease")}</option>
            </select>
          </Field>
        </section>
      )}

      {warnings.map((w) => (
        <div key={w} className="warn">
          {w}
        </div>
      ))}
      <div className="conn-hint dim small">{t("config.applyHint")}</div>
    </div>
  );
}

/** Label (with the env var it maps to), control, hint and error for one setting. */
function Field({ k, error, children }: { k: FieldKey; error: string | null; children: ReactNode }) {
  const { t } = useTranslation();
  return (
    <div className={"config-field" + (error ? " invalid" : "")}>
      <label className="form-label">
        {t(`config.field.${k}`)} <span className="config-key">{k}</span>
      </label>
      {children}
      {error ? (
        <div className="config-field-error">{error}</div>
      ) : (
        <div className="config-hint dim small">{t(`config.hint.${k}`)}</div>
      )}
    </div>
  );
}
