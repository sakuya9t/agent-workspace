import { chmodSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { homedir, hostname } from "node:os";
import { dirname, join } from "node:path";
import type { Plugin } from "vite";
import {
  APPLIED_FILE_NAME,
  CONFIG_ERROR_TEXT,
  CONFIG_FILE_NAME,
  CONFIG_KEYS,
  type ConfigValues,
  configRevision,
  isConfigKey,
  isSecretKey,
  parseConfigFile,
  pendingKeys,
  renderConfigFile,
  validateConfig,
} from "./src/startupConfig.ts";

// `/__asm/config` — the UI host's saved startup configuration, served by the
// Vite dev server itself so the web client's Config dialog can edit the machine
// it is served from even when no daemon runs there (a UI-only gateway). Daemons
// serve their own host's file at `GET/PUT /api/config`; this is the same
// contract (see crates/daemon/src/startup_config.rs) over the same file format.
//
// Anyone who can load this UI can already drive the daemon it proxies to
// (loopback-trusted or token-injected), so the endpoint adds no reach — but it
// is a same-origin API: writes must be JSON (forcing a CORS preflight a foreign
// page cannot pass) and carry a matching Origin when the browser sends one.
// Secrets are write-only, as on the daemon.

export const CONFIG_API_PATH = "/__asm/config";

type Env = Record<string, string | undefined>;

/** The same locations scripts/_asm_common.sh derives for this install. */
export function configPaths(env: Env): { path: string; appliedPath: string } {
  const dataDir = env.ASM_DATA_DIR || join(homedir(), ".local", "share", "asm");
  const runtimeDir = env.ASM_RUNTIME_DIR || join(env.XDG_RUNTIME_DIR || "/tmp", "asm");
  return {
    path: join(dataDir, CONFIG_FILE_NAME),
    appliedPath: join(runtimeDir, APPLIED_FILE_NAME),
  };
}

function readOptional(path: string): string | null {
  try {
    return readFileSync(path, "utf8");
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw e;
  }
}

interface Snapshot {
  exists: boolean;
  revision: string | null;
  values: ConfigValues;
  pending: string[] | null;
}

/** The file's values, falling back to this process's env (saved > env > default). */
function load(env: Env): Snapshot {
  const { path, appliedPath } = configPaths(env);
  const content = readOptional(path);
  const saved = content === null ? null : parseConfigFile(content);
  const values: ConfigValues = {};
  for (const key of CONFIG_KEYS) {
    const v = saved && key in saved ? saved[key] : env[key];
    if (v !== undefined) values[key] = v;
  }
  const applied = readOptional(appliedPath);
  return {
    exists: content !== null,
    revision: content === null ? null : configRevision(content),
    values,
    pending: saved && applied !== null ? pendingKeys(saved, parseConfigFile(applied)) : null,
  };
}

function view(env: Env, snap: Snapshot) {
  const values: Record<string, string> = {};
  const secrets: Record<string, boolean> = {};
  for (const key of CONFIG_KEYS) {
    const v = snap.values[key];
    if (isSecretKey(key)) secrets[key] = !!v;
    else if (v !== undefined) values[key] = v;
  }
  return {
    source: "ui",
    hostname: hostname(),
    path: configPaths(env).path,
    exists: snap.exists,
    revision: snap.revision,
    values,
    secrets,
    pending: snap.pending,
    // Launched by scripts/start.sh (which reads this file); a hand-run
    // `npm run dev` only picks the settings up once started that way.
    managed: env.ASM_UI_MANAGED === "1",
  };
}

/** Replace the file atomically, owner-only — it can hold relay keys and a token. */
function writeAtomic(path: string, content: string) {
  mkdirSync(dirname(path), { recursive: true });
  const tmp = `${path}.tmp.${process.pid}`;
  try {
    writeFileSync(tmp, content, { mode: 0o600 });
    chmodSync(tmp, 0o600);
    renameSync(tmp, path);
  } catch (e) {
    rmSync(tmp, { force: true });
    throw e;
  }
}

// No parameter properties: `npm test` runs this file under Node's type
// stripping, which only accepts erasable TypeScript.
class HttpError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** Apply `patch` to the current values and write them. Mirrors the daemon's `Store::save`. */
export function saveConfig(env: Env, revision: string | null, patch: Record<string, unknown>) {
  const current = load(env);
  if (current.revision !== revision) {
    throw new HttpError(409, "the saved config changed since it was loaded — reload it and reapply your edits");
  }
  const values: ConfigValues = { ...current.values };
  for (const [key, value] of Object.entries(patch)) {
    if (!isConfigKey(key)) throw new HttpError(400, `${key} is not a startup setting`);
    if (typeof value !== "string") throw new HttpError(400, `${key}: expected a string`);
    values[key] = value.trim();
  }
  const [first] = validateConfig(values);
  if (first) throw new HttpError(400, `${first.key}: ${CONFIG_ERROR_TEXT[first.code]}`);
  writeAtomic(configPaths(env).path, renderConfigFile(values));
  return view(env, load(env));
}

function send(res: ServerResponse, status: number, body: unknown) {
  res.statusCode = status;
  res.setHeader("content-type", "application/json");
  res.setHeader("cache-control", "no-store");
  res.end(JSON.stringify(body));
}

function readBody(req: IncomingMessage, limit = 64 * 1024): Promise<string> {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks: Buffer[] = [];
    req.on("data", (c: Buffer) => {
      size += c.length;
      if (size > limit) {
        reject(new HttpError(413, "request body too large"));
        req.destroy();
      } else chunks.push(c);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

/** Reject a write a foreign page could have sent from the user's browser. */
function assertSameOrigin(req: IncomingMessage) {
  if (!String(req.headers["content-type"] ?? "").toLowerCase().startsWith("application/json")) {
    throw new HttpError(415, "expected application/json");
  }
  const site = req.headers["sec-fetch-site"];
  if (site && site !== "same-origin" && site !== "none") {
    throw new HttpError(403, "cross-site config writes are refused");
  }
  const origin = req.headers.origin;
  if (origin) {
    let originHost: string | null = null;
    try {
      originHost = new URL(origin).host;
    } catch {
      /* malformed */
    }
    if (originHost !== req.headers.host) throw new HttpError(403, "cross-origin config writes are refused");
  }
}

/** The connect middleware, exported for tests. */
export function createConfigHandler(env: Env = process.env) {
  return async (req: IncomingMessage, res: ServerResponse, next: (err?: unknown) => void) => {
    const path = (req.url ?? "").split("?")[0];
    if (path !== CONFIG_API_PATH) return next();
    try {
      if (req.method === "GET") return send(res, 200, view(env, load(env)));
      if (req.method === "PUT") {
        assertSameOrigin(req);
        let body: { revision?: unknown; values?: unknown };
        try {
          body = JSON.parse(await readBody(req));
        } catch (e) {
          if (e instanceof HttpError) throw e;
          throw new HttpError(400, "invalid JSON body");
        }
        const revision = body.revision ?? null;
        if (revision !== null && typeof revision !== "string") throw new HttpError(400, "revision: expected a string");
        if (!body.values || typeof body.values !== "object" || Array.isArray(body.values)) {
          throw new HttpError(400, "values: expected an object");
        }
        return send(res, 200, saveConfig(env, revision, body.values as Record<string, unknown>));
      }
      res.setHeader("allow", "GET, PUT");
      return send(res, 405, { error: "method not allowed" });
    } catch (e) {
      if (e instanceof HttpError) return send(res, e.status, { error: e.message });
      return send(res, 500, { error: `startup config: ${(e as Error).message}` });
    }
  };
}

/** Vite plugin: mount the endpoint on the dev (and preview) server. */
export function startupConfigApi(env: Env = process.env): Plugin {
  const handler = createConfigHandler(env);
  return {
    name: "asm-startup-config-api",
    configureServer(server) {
      server.middlewares.use((req, res, next) => void handler(req, res, next));
    },
    configurePreviewServer(server) {
      server.middlewares.use((req, res, next) => void handler(req, res, next));
    },
  };
}
