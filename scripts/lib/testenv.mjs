// Hermetic sandbox for the e2e/smoke scripts.
//
// Why this exists: on 2026-07-12 a test run destroyed six live prod sessions.
// The scripts each rolled their own isolation, and most got it wrong — they
// spread `...process.env` into the daemon they spawned, and this dev host
// exports ASMUX_SOCK / ASM_RUNTIME_DIR / ASM_DATA_DIR / ASM_BIND globally. The
// daemon resolves ASMUX_SOCK *first* (config.rs), so a test's private runtime
// dir was ignored, its asmux unlinked the real holder's socket, and the real
// holder's PTYs were orphaned. A private TCP port does not save you: the
// collision is on the unix socket and the data dir, not the port.
//
// So: never hand-roll the env. Take a sandbox from here.
//
//   import { createSandbox } from "./lib/testenv.mjs";
//   const sb = await createSandbox("my-test");     // tmpdir + free port, nothing started
//   await sb.startAsmux();                         // optional: only for sidecar/durability tests
//   await sb.startDaemon();                        // waits for /health
//   ... sb.http, sb.base, sb.api(), sb.ws(id) ...
//   sb.cleanup();                                  // kills every child, removes the tmpdir
//
// Everything the sandbox spawns is confined to a fresh mkdtemp: data dir,
// runtime dir, asmux socket, and cwd. Inherited ASM_*/ASMUX_* are stripped, and
// XDG_RUNTIME_DIR / XDG_DATA_HOME / XDG_CONFIG_HOME are repointed into the
// sandbox so that even *default* path resolution cannot escape it.

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, existsSync, openSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "node:net";

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const DAEMON_BIN = join(ROOT, "target", "debug", "asm-daemon");
export const ASMUX_BIN = join(ROOT, "target", "debug", "asmux");
/** The built web client. Serve it from the daemon (ASM_STATIC_DIR) for same-origin UI tests. */
export const CLIENT_DIST = join(ROOT, "client", "dist");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** A 1x1 PNG — valid magic bytes, so the daemon's image sniff accepts it. */
export const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

function findChrome() {
  const candidates = [
    process.env.CHROME_BIN,
    "/opt/google/chrome/chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ].filter(Boolean);
  for (const c of candidates) if (existsSync(c)) return c;
  throw new Error("no Chrome found — install one or set CHROME_BIN");
}

/** Minimal CDP client over the DevTools WebSocket (no puppeteer dependency). */
export function cdpConnect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  const pending = new Map();
  let idc = 1;
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
    }
  };
  const ready = new Promise((res) => (ws.onopen = res));
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = idc++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  return { ws, ready, send };
}

/** The real, live installation. Nothing a test spawns may touch these. */
export function prodPaths() {
  const xdgRun = process.env.XDG_RUNTIME_DIR || `/run/user/${process.getuid?.() ?? 1000}`;
  return {
    socket: resolve(xdgRun, "asm", "asmux.sock"),
    runtimeDir: resolve(xdgRun, "asm"),
    dataDir: resolve(homedir(), ".local", "share", "asm"),
    ports: [4600, 4700], // daemon, relay
  };
}

/** Ask the kernel for a port nobody is using, rather than guessing from the pid. */
export function freePort() {
  return new Promise((res, rej) => {
    const srv = createServer();
    srv.on("error", rej);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => res(port));
    });
  });
}

/**
 * process.env minus every `ASM_` and `ASMUX_` key. The ambient shell on a dev
 * box exports prod config; inheriting it is what caused the incident.
 */
export function cleanEnv() {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      ([k]) => !k.startsWith("ASM_") && !k.startsWith("ASMUX_"),
    ),
  );
}

/**
 * Child env for a daemon/asmux spawned outside createSandbox() — e.g. the
 * multi-daemon topologies in relay-test / gateway-test.
 *
 * Strips the ambient `ASM_` / `ASMUX_` vars, then pins ASMUX_SOCK to the child's own
 * runtime dir. That last part is the one that bit us: the daemon resolves
 * ASMUX_SOCK before falling back to ASM_RUNTIME_DIR (config.rs), so a child
 * given only a private ASM_RUNTIME_DIR still inherited the prod socket. Callers
 * that forget ASM_RUNTIME_DIR now get an exception instead of prod.
 */
export function hermeticChildEnv(env = {}) {
  const runtimeDir = env.ASM_RUNTIME_DIR;
  const socket = env.ASMUX_SOCK ?? (runtimeDir ? join(runtimeDir, "asmux.sock") : null);
  if (!socket) {
    throw new Error(
      "hermeticChildEnv: pass ASM_RUNTIME_DIR (or an explicit ASMUX_SOCK) so the child " +
        "cannot fall back to the prod holder socket",
    );
  }
  const prod = prodPaths();
  if (resolve(socket) === prod.socket) {
    throw new Error(`refusing to point a test child at the PROD asmux socket ${socket}`);
  }
  if (env.ASM_DATA_DIR && resolve(env.ASM_DATA_DIR) === prod.dataDir) {
    throw new Error(`refusing to point a test child at the PROD data dir ${env.ASM_DATA_DIR}`);
  }
  return { ...cleanEnv(), ...env, ASMUX_SOCK: socket };
}

export async function createSandbox(name = "asm-test") {
  const tmp = mkdtempSync(join(tmpdir(), `${name}-`));
  const dataDir = join(tmp, "data");
  const configDir = join(tmp, "config");
  const runDir = join(tmp, "run");
  const xdgDir = join(tmp, "xdg");
  const cwd = join(tmp, "cwd");
  for (const d of [dataDir, configDir, runDir, xdgDir, cwd]) mkdirSync(d, { recursive: true });

  const socket = join(runDir, "asmux.sock");
  const port = await freePort();
  const base = `127.0.0.1:${port}`;
  const prod = prodPaths();

  // Tripwire. If any of this is ever false we are about to touch the real
  // install, and it is better to fail the test than to eat someone's sessions.
  const guard = () => {
    const under = (p) => resolve(p).startsWith(resolve(tmp) + "/");
    if (!under(socket) || !under(dataDir) || !under(runDir)) {
      throw new Error(`sandbox escaped its tmpdir: socket=${socket} data=${dataDir}`);
    }
    if (resolve(socket) === prod.socket) throw new Error(`refusing to bind the PROD asmux socket ${socket}`);
    if (resolve(dataDir) === prod.dataDir) throw new Error(`refusing to use the PROD data dir ${dataDir}`);
    if (prod.ports.includes(port)) throw new Error(`refusing to bind the PROD port ${port}`);
  };
  guard();

  const procs = [];

  /** Hermetic child env: clean base, private XDG, explicit ASM_* on top. */
  const env = (extra = {}) => ({
    ...cleanEnv(),
    HOME: process.env.HOME, // keep: git/agent binaries need it
    XDG_RUNTIME_DIR: xdgDir,
    XDG_DATA_HOME: join(xdgDir, "data"),
    XDG_CONFIG_HOME: join(xdgDir, "config"),
    ASM_DATA_DIR: dataDir,
    ASM_CONFIG_DIR: configDir,
    ASM_RUNTIME_DIR: runDir,
    ASMUX_SOCK: socket, // explicit — never inherit, never infer
    ASM_LOG: process.env.ASM_TEST_LOG ?? "info",
    ...extra,
  });

  function startProc(procName, bin, extraEnv = {}, { detached = false } = {}) {
    guard();
    if (!existsSync(bin)) {
      throw new Error(`missing binary ${bin} — run \`cargo build\` first`);
    }
    const log = openSync(join(tmp, `${procName}.log`), "a");
    const child = spawn(bin, [], {
      env: env(extraEnv),
      stdio: ["ignore", log, log],
      detached,
    });
    procs.push({ name: procName, child });
    return child;
  }

  const sb = {
    tmp,
    cwd,
    dataDir,
    runDir,
    socket,
    port,
    base,
    http: `http://${base}`,
    env,
    startProc,
    logPath: (n) => join(tmp, `${n}.log`),

    /** The holder. Only sidecar/durability tests need this. Detached: it must outlive the daemon. */
    async startAsmux() {
      startProc("asmux", ASMUX_BIN, {}, { detached: true });
      for (let i = 0; i < 50 && !existsSync(socket); i++) await sleep(100);
      if (!existsSync(socket)) throw new Error(`asmux socket never appeared at ${socket}`);
      return socket;
    },

    /** Spawn a daemon and wait for /health. `backend` defaults to native (no holder needed). */
    async startDaemon(procName = "daemon", extraEnv = {}) {
      startProc(procName, DAEMON_BIN, { ASM_BIND: base, ...extraEnv });
      return sb.waitHealth();
    },

    /**
     * A daemon that also *serves the built web client*, so the UI is same-origin
     * on loopback (baseUrl="" ⇒ loopback trust ⇒ no token). This is what the
     * headless-Chrome tests drive; it replaces the hand-rolled "start a THROWAWAY
     * daemon on 4671" recipe those tests used to carry in their header comments.
     */
    async startAppDaemon(procName = "daemon", extraEnv = {}) {
      if (!existsSync(join(CLIENT_DIST, "index.html"))) {
        throw new Error(`missing ${CLIENT_DIST}/index.html — run \`cd client && npm run build\``);
      }
      return sb.startDaemon(procName, { ASM_STATIC_DIR: CLIENT_DIST, ...extraEnv });
    },

    /**
     * Launch headless Chrome (profile inside the sandbox) and return its DevTools
     * endpoint. Use this when you need to build your own CDP client — e.g. one
     * that taps console logs or auto-accepts dialogs. Killed by cleanup().
     *
     * `extraArgs` are appended to the command line — e.g.
     * `--ignore-certificate-errors` to stand in for a user who has accepted a
     * self-signed daemon cert (a background fetch to an untrusted cert fails the
     * same opaque way as a dead host, and never offers an interstitial).
     */
    async launchChrome(extraArgs = []) {
      const bin = findChrome();
      const cdpPort = await freePort();
      const profile = join(tmp, "chrome");
      mkdirSync(profile, { recursive: true });
      const log = openSync(join(tmp, "chrome.log"), "a");
      const child = spawn(
        bin,
        [
          "--headless=new",
          "--disable-gpu",
          "--no-sandbox",
          "--disable-dev-shm-usage",
          "--no-first-run",
          "--noerrdialogs",
          ...extraArgs,
          `--remote-debugging-port=${cdpPort}`,
          `--user-data-dir=${profile}`,
          "about:blank",
        ],
        { env: cleanEnv(), stdio: ["ignore", log, log] },
      );
      procs.push({ name: "chrome", child });

      let wsUrl = null;
      for (let i = 0; i < 60 && !wsUrl; i++) {
        try {
          const r = await fetch(`http://127.0.0.1:${cdpPort}/json/version`);
          wsUrl = (await r.json()).webSocketDebuggerUrl ?? null;
        } catch {
          /* not up yet */
        }
        if (!wsUrl) await sleep(250);
      }
      if (!wsUrl) throw new Error(`chrome devtools never came up (see ${sb.logPath("chrome")})`);
      return { port: cdpPort, wsUrl };
    },

    /**
     * Headless Chrome plus a connected CDP client and page helpers. The common
     * case; reach for launchChrome() only if you need a custom client.
     */
    async startChrome(extraArgs = []) {
      const { port: cdpPort, wsUrl } = await sb.launchChrome(extraArgs);
      const conn = cdpConnect(wsUrl);
      await conn.ready;

      /** Open `url` in a new tab and attach a flat CDP session to it. */
      conn.openPage = async (url) => {
        const { targetId } = await conn.send("Target.createTarget", { url });
        const { sessionId } = await conn.send("Target.attachToTarget", {
          targetId,
          flatten: true,
        });
        const S = (method, params) => conn.send(method, params, sessionId);
        await S("Runtime.enable");
        await S("DOM.enable");
        await S("Page.enable");

        const evalJs = async (expression) => {
          const { result, exceptionDetails } = await S("Runtime.evaluate", {
            expression,
            returnByValue: true,
            awaitPromise: true,
          });
          // Surface page exceptions instead of silently yielding undefined — a
          // failed check with no reason is a bad afternoon.
          if (exceptionDetails) throw new Error(`${exceptionDetails.text} :: ${expression}`);
          return result.value;
        };
        const waitFor = async (expression, ms = 12000) => {
          const t0 = Date.now();
          while (Date.now() - t0 < ms) {
            if (await evalJs(expression)) return true;
            await sleep(300);
          }
          return false;
        };
        return { targetId, sessionId, S, evalJs, waitFor };
      };

      conn.port = cdpPort;
      return conn;
    },

    async waitHealth(timeoutMs = 15000) {
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        try {
          const res = await fetch(`${sb.http}/health`);
          if (res.ok) return await res.json();
        } catch {
          /* not up yet */
        }
        await sleep(150);
      }
      throw new Error(`daemon /health did not come up on ${base} — see ${sb.logPath("daemon")}`);
    },

    stop(procName) {
      const p = procs.find((x) => x.name === procName && !x.child.killed);
      if (!p) return;
      try {
        process.kill(p.child.pid, "SIGTERM");
      } catch {
        /* already gone */
      }
    },

    async waitExit(procName, timeoutMs = 5000) {
      const p = procs.find((x) => x.name === procName);
      if (!p) return;
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        if (p.child.exitCode !== null || p.child.signalCode !== null) return;
        await sleep(50);
      }
    },

    /** JSON helper against this sandbox's daemon. */
    async api(path, init) {
      const res = await fetch(sb.http + path, {
        ...init,
        headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
      });
      const text = await res.text();
      if (!res.ok) throw new Error(`${path} -> ${res.status} ${text}`);
      return text ? JSON.parse(text) : null;
    },

    /** Open a stream WS and accumulate output. */
    ws(id) {
      const sock = new WebSocket(`ws://${base}/api/sessions/${id}/stream`);
      sock.binaryType = "arraybuffer";
      const state = { buf: "", ws: sock };
      sock.onmessage = (ev) => {
        state.buf +=
          typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString("utf8");
      };
      return state;
    },

    cleanup() {
      for (const { child } of procs) {
        try {
          // Negative pid kills the whole group for detached children (asmux).
          process.kill(child.pid, "SIGKILL");
        } catch {
          /* gone */
        }
      }
      try {
        rmSync(tmp, { recursive: true, force: true });
      } catch {
        /* best effort */
      }
    },
  };

  return sb;
}

/** Small PASS/FAIL harness shared by the scripts. */
export function checker() {
  const state = { failures: 0 };
  state.check = (name, cond, extra) => {
    const ok = !!cond;
    console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`);
    if (!ok) state.failures++;
    return ok;
  };
  state.report = (msg) => {
    console.log(
      state.failures === 0 ? `\nALL PASS — ${msg}` : `\n${state.failures} FAILURE(S)`,
    );
    return state.failures === 0;
  };
  return state;
}

export { sleep };
