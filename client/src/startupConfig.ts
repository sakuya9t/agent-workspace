// A host's saved startup configuration (`<data_dir>/startup.conf`): the
// settings `scripts/start.sh` starts from, edited by the Config dialog.
//
// Shared by the browser (the dialog's client-side checks) and the Vite dev
// server (`vite.config-api.ts`, which serves the UI host's file), so it must
// stay free of DOM, React, and i18n imports. The same rules are implemented in
// `crates/daemon/src/startup_config.rs` and `scripts/_asm_common.sh`; keep the
// three in step.

/**
 * Every setting, in file order. The keys are the scripts' env var names. Three
 * independent components — the daemon (with its session holder), a bundled
 * relay, and the managed web UI — are switched by their `ASM_RUN_*` keys; any
 * non-empty subset is valid.
 */
export const CONFIG_KEYS = [
  "ASM_PROFILE",
  "ASM_RUN_DAEMON",
  "ASM_BIND",
  "ASM_NODE_LABEL",
  "ASM_ENROLLMENT_TOKEN",
  "ASM_RELAY_URL",
  "ASM_RELAY_KEY",
  "ASM_RELAY_DOWNSTREAMS",
  "ASM_RUN_RELAY",
  "ASM_RELAY_BIND",
  "ASM_RELAY_KEYS",
  "ASM_RUN_UI",
  "ASM_UI_HOST",
  "ASM_UI_PORT",
  "ASM_UI_ALLOWED_HOSTS",
  "ASM_UI_DAEMON",
  "ASM_UI_DAEMON_TOKEN",
] as const;

export type ConfigKey = (typeof CONFIG_KEYS)[number];
export type ConfigValues = Partial<Record<ConfigKey, string>>;

/** Write-only over the API: a read reports only whether each is set. */
export const SECRET_KEYS: readonly ConfigKey[] = [
  "ASM_ENROLLMENT_TOKEN",
  "ASM_RELAY_KEY",
  "ASM_RELAY_KEYS",
  "ASM_UI_DAEMON_TOKEN",
];

export const isConfigKey = (k: string): k is ConfigKey =>
  (CONFIG_KEYS as readonly string[]).includes(k);
export const isSecretKey = (k: string): boolean => (SECRET_KEYS as readonly string[]).includes(k);

/** What the scripts use for an empty/missing value. */
export const CONFIG_DEFAULTS: Record<ConfigKey, string> = {
  ASM_PROFILE: "debug",
  ASM_RUN_DAEMON: "1",
  ASM_BIND: "127.0.0.1:4600",
  ASM_NODE_LABEL: "",
  ASM_ENROLLMENT_TOKEN: "",
  ASM_RELAY_URL: "",
  ASM_RELAY_KEY: "",
  ASM_RELAY_DOWNSTREAMS: "",
  ASM_RUN_RELAY: "0",
  ASM_RELAY_BIND: "0.0.0.0:4700",
  ASM_RELAY_KEYS: "",
  ASM_RUN_UI: "1",
  ASM_UI_HOST: "127.0.0.1",
  ASM_UI_PORT: "5273",
  ASM_UI_ALLOWED_HOSTS: "",
  ASM_UI_DAEMON: "",
  ASM_UI_DAEMON_TOKEN: "",
};

/** The component switches. */
export const COMPONENT_KEYS = ["ASM_RUN_DAEMON", "ASM_RUN_RELAY", "ASM_RUN_UI"] as const;
export type ComponentKey = (typeof COMPONENT_KEYS)[number];
const BOOL_KEYS: readonly ConfigKey[] = COMPONENT_KEYS;

/** Whether a component runs; an empty/missing switch means its default. */
export const componentOn = (values: ConfigValues, key: ComponentKey): boolean =>
  (values[key] || CONFIG_DEFAULTS[key]) === "1";
const MAX_VALUE_LEN = 4096;
/** Shortest enrollment token accepted: it is the LAN's only gate to enrolling. */
export const MIN_ENROLLMENT_TOKEN_LEN = 16;

export const CONFIG_FILE_NAME = "startup.conf";
export const APPLIED_FILE_NAME = "startup.applied";

const HEADER = [
  "# Agent Session Manager — saved startup settings for this install.",
  "# scripts/start.sh and scripts/restart-daemon.sh start from these on every run;",
  "# command-line flags override them and are saved back. Also editable from the",
  "# web UI (Config). One KEY=value per line, no quotes; an empty value means the",
  "# default.",
].join("\n");

/** Parse the file; comments, malformed lines and unknown keys are skipped. */
export function parseConfigFile(text: string): ConfigValues {
  const out: ConfigValues = {};
  for (const raw of text.split("\n")) {
    const line = raw.replace(/\r$/, "");
    const trimmed = line.trimStart();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq < 0) continue;
    const key = line.slice(0, eq).trim();
    if (isConfigKey(key)) out[key] = line.slice(eq + 1).trim();
  }
  return out;
}

/** Render every key in file order; a key with no value is written empty. */
export function renderConfigFile(values: ConfigValues): string {
  return HEADER + "\n" + CONFIG_KEYS.map((k) => `${k}=${values[k] ?? ""}`).join("\n") + "\n";
}

/**
 * Opaque change token: FNV-1a 64 over the UTF-8 bytes, as 16 hex digits — the
 * daemon computes the same, so either server accepts the other's token.
 */
export function configRevision(text: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(text)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}

/** Keys whose saved value differs from the applied one. Absent ≡ empty. */
export function pendingKeys(saved: ConfigValues, applied: ConfigValues): ConfigKey[] {
  return CONFIG_KEYS.filter((k) => (saved[k] ?? "") !== (applied[k] ?? ""));
}

/** Why a value or combination is refused; the dialog maps codes to text. */
export type ConfigErrorCode =
  | "multiline"
  | "pipe"
  | "tooLong"
  | "profile"
  | "bool"
  | "socketAddr"
  | "label"
  | "wsUrl"
  | "httpUrl"
  | "token"
  | "enrollmentToken"
  | "keyList"
  | "downstreams"
  | "uiHost"
  | "port"
  | "hostList"
  | "noComponent"
  | "relayNeedsKeys"
  | "registerNeedsKey";

export interface ConfigError {
  key: ConfigKey;
  code: ConfigErrorCode;
}

const PORT_RE = /^\d{1,5}$/;
const isPort = (s: string) => PORT_RE.test(s) && +s >= 1 && +s <= 65535;

/** `IP:port` exactly as the daemon parses a `SocketAddr`, with a non-zero port. */
function isSocketAddr(s: string): boolean {
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3}):(\d{1,5})$/.exec(s);
  if (v4) return v4.slice(1, 5).every((o) => +o <= 255 && String(+o) === o) && isPort(v4[5]);
  const v6 = /^\[([0-9A-Fa-f:.]+)\]:(\d{1,5})$/.exec(s);
  return !!v6 && v6[1].includes(":") && isPort(v6[2]);
}

const hasSchemeAndAuthority = (s: string, schemes: string[]) =>
  !/\s/.test(s) &&
  schemes.some((p) => s.startsWith(p) && s.length > p.length && s[p.length] !== "/");
const isToken = (s: string) => !/[\s,]/.test(s);
const HOSTNAME_RE = /^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$/;
const HOST_PORT_RE = /^(\[[0-9A-Fa-f:.]+\]|[A-Za-z0-9.-]+):(\d{1,5})$/;
const list = (s: string) => s.split(",").map((x) => x.trim());

/** One value on its own. Empty always passes: it means "the default". */
export function validateValue(key: ConfigKey, value: string): ConfigErrorCode | null {
  if (/[\u0000-\u001f\u007f-\u009f]/.test(value)) return "multiline";
  if (value.includes("|")) return "pipe";
  if (new TextEncoder().encode(value).length > MAX_VALUE_LEN) return "tooLong";
  if (!value) return null;
  switch (key) {
    case "ASM_PROFILE":
      return value === "debug" || value === "release" ? null : "profile";
    case "ASM_BIND":
    case "ASM_RELAY_BIND":
      return isSocketAddr(value) ? null : "socketAddr";
    case "ASM_NODE_LABEL":
      return [...value].length <= 100 ? null : "label";
    case "ASM_RELAY_URL":
      return hasSchemeAndAuthority(value, ["ws://", "wss://"]) ? null : "wsUrl";
    case "ASM_UI_DAEMON":
      return hasSchemeAndAuthority(value, ["http://", "https://"]) ? null : "httpUrl";
    case "ASM_RELAY_KEY":
    case "ASM_UI_DAEMON_TOKEN":
      return isToken(value) ? null : "token";
    case "ASM_ENROLLMENT_TOKEN":
      return isToken(value) && value.length >= MIN_ENROLLMENT_TOKEN_LEN ? null : "enrollmentToken";
    case "ASM_RELAY_KEYS":
      return list(value).every((k) => k && isToken(k)) ? null : "keyList";
    case "ASM_RELAY_DOWNSTREAMS":
      return list(value).every((d) => {
        const m = HOST_PORT_RE.exec(d);
        return !!m && isPort(m[2]);
      })
        ? null
        : "downstreams";
    case "ASM_UI_HOST":
      return /^[A-Za-z0-9.:\-_[\]*]+$/.test(value) ? null : "uiHost";
    case "ASM_UI_PORT":
      return isPort(value) ? null : "port";
    case "ASM_UI_ALLOWED_HOSTS":
      return list(value).every((h) => HOSTNAME_RE.test(h)) ? null : "hostList";
    default:
      return BOOL_KEYS.includes(key) ? (value === "0" || value === "1" ? null : "bool") : null;
  }
}

/**
 * Every value, then the dependencies between settings: the components do not
 * depend on each other — only settings of a component that runs are checked.
 * `secretSet` says whether a secret key will hold a value once saved (the
 * dialog knows a secret is set without knowing it).
 */
export function validateConfig(
  values: ConfigValues,
  secretSet: (key: ConfigKey) => boolean = (k) => !!values[k],
): ConfigError[] {
  const errors: ConfigError[] = [];
  for (const key of CONFIG_KEYS) {
    const code = validateValue(key, values[key] ?? "");
    if (code) errors.push({ key, code });
  }
  const daemon = componentOn(values, "ASM_RUN_DAEMON");
  const relay = componentOn(values, "ASM_RUN_RELAY");
  const ui = componentOn(values, "ASM_RUN_UI");
  if (!daemon && !relay && !ui) errors.push({ key: "ASM_RUN_DAEMON", code: "noComponent" });
  if (relay && !secretSet("ASM_RELAY_KEYS")) errors.push({ key: "ASM_RELAY_KEYS", code: "relayNeedsKeys" });
  if (daemon && values.ASM_RELAY_URL && !secretSet("ASM_RELAY_KEY")) {
    errors.push({ key: "ASM_RELAY_KEY", code: "registerNeedsKey" });
  }
  return errors;
}

/** Plain-English text for server-side responses (the dialog uses i18n). */
export const CONFIG_ERROR_TEXT: Record<ConfigErrorCode, string> = {
  multiline: "must be a single line of text",
  pipe: "may not contain `|`",
  tooLong: "is too long",
  profile: "expected `debug` or `release`",
  bool: "expected 0 or 1",
  socketAddr: "expected an IP address and port, e.g. 0.0.0.0:4600",
  label: "must be at most 100 characters",
  wsUrl: "expected a ws:// or wss:// relay URL",
  httpUrl: "expected an http:// or https:// daemon URL",
  token: "may not contain spaces or commas",
  enrollmentToken: `expected at least ${MIN_ENROLLMENT_TOKEN_LEN} characters, without spaces or commas`,
  keyList: "expected comma-separated keys without spaces",
  downstreams: "expected comma-separated host:port entries",
  uiHost: "expected a hostname or IP address",
  port: "expected a port between 1 and 65535",
  hostList: "expected comma-separated exact hostnames (no scheme, port, path, or wildcard)",
  noComponent: "enable at least one of the daemon, relay, or web UI",
  relayNeedsKeys: "a relay needs at least one access key",
  registerNeedsKey: "registering to a relay needs its access key",
};
