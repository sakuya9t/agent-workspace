// Relay end-to-end test — the headline proof that a daemon reachable ONLY by
// dialing out (a private host behind NAT) is fully controllable through the
// relay (docs/connectivity-execution-plan.md, R1+R2).
//
//   node scripts/relay-test.mjs
//
// It orchestrates everything itself, all on loopback:
//   1. start asm-relay with an access key
//   2. start a daemon configured to register OUTBOUND to the relay
//      (ASM_RELAY_URL/KEY) — it is reached only through /n/<node_id>
//   3. discover it in the relay's /nodes (online) and learn its node_id
//   4. enroll a device THROUGH the relay, then drive the full session loop
//      through /n/<node_id>: create -> attach WS -> marker echo -> stop
//   5. security asserts:
//        - a relayed request with NO device token is 401 even though it lands
//          on the daemon's loopback tunnel socket (loopback-trust regression)
//        - GET /api/auth/enrollment-token through the relay, WITH a valid token,
//          is 403 — enrollment tokens are never retrievable through the relay
//   6. kill + restart the relay; the daemon re-registers and the loop works
//
// Requires the built binaries (`cargo build`) and Node 18+ (global WebSocket).

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync, openSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { hermeticChildEnv } from "./lib/testenv.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const DAEMON = join(ROOT, "target", "debug", "asm-daemon");
const RELAY = join(ROOT, "target", "debug", "asm-relay");

const KEY = "relay-test-key";
const RELAY_PORT = 4800 + (process.pid % 100);
const DAEMON_PORT = 4950 + (process.pid % 100);
const RELAY_HTTP = `http://127.0.0.1:${RELAY_PORT}`;
const RELAY_WS = `ws://127.0.0.1:${RELAY_PORT}`;
const DAEMON_HTTP = `http://127.0.0.1:${DAEMON_PORT}`;

let failures = 0;
function check(name, cond, extra) {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra !== undefined ? "  " + extra : ""}`);
  if (!ok) failures++;
  return ok;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const tmp = mkdtempSync(join(tmpdir(), "asm-relay-"));
const runDir = join(tmp, "run");
const dataDir = join(tmp, "data");
const procs = [];
let nodeId = null;

function startProc(name, bin, env) {
  const log = openSync(join(tmp, `${name}.log`), "a");
  const child = spawn(bin, [], {
    // Never inherit the dev host's ASM_*/ASMUX_* — they point at the live install.
    env: hermeticChildEnv(env),
    stdio: ["ignore", log, log],
  });
  procs.push({ name, child });
  return child;
}

function stop(name) {
  const p = procs.find((x) => x.name === name && !x.child.killed);
  if (!p) return;
  try {
    process.kill(p.child.pid, "SIGTERM");
  } catch {
    /* already gone */
  }
}

async function waitExit(name, timeoutMs) {
  const p = procs.find((x) => x.name === name);
  if (!p) return;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (p.child.exitCode !== null || p.child.signalCode !== null) return;
    await sleep(50);
  }
}

function cleanup() {
  for (const { child } of procs) {
    try {
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
}

// A relay-routed fetch: appends the relay key, optionally a bearer token.
async function relay(path, { token, method, body } = {}) {
  const sep = path.includes("?") ? "&" : "?";
  const url = `${RELAY_HTTP}${path}${sep}relay_key=${KEY}`;
  const headers = {};
  if (body) headers["content-type"] = "application/json";
  if (token) headers["authorization"] = `Bearer ${token}`;
  return fetch(url, { method, headers, body: body ? JSON.stringify(body) : undefined });
}

async function waitRelayUp(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${RELAY_HTTP}/nodes?relay_key=${KEY}`);
      if (res.ok) return;
    } catch {
      /* not up */
    }
    await sleep(100);
  }
  throw new Error("relay did not come up");
}

async function waitDaemonHealth(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${DAEMON_HTTP}/health`);
      if (res.ok) return await res.json();
    } catch {
      /* not up */
    }
    await sleep(150);
  }
  throw new Error("daemon /health did not come up");
}

// Poll /nodes until a node is online; returns its entry.
async function waitNodeOnline(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${RELAY_HTTP}/nodes?relay_key=${KEY}`);
      if (res.ok) {
        const { nodes } = await res.json();
        const online = nodes.find((n) => n.online);
        if (online) return online;
      }
    } catch {
      /* retry */
    }
    await sleep(100);
  }
  throw new Error("node never registered online");
}

function wsCollectRelay(sid, token) {
  const url = `${RELAY_WS}/n/${nodeId}/api/sessions/${sid}/stream?access_token=${encodeURIComponent(token)}&relay_key=${KEY}`;
  const ws = new WebSocket(url);
  ws.binaryType = "arraybuffer";
  const state = { buf: "", ws };
  ws.onmessage = (ev) => {
    state.buf += typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString("utf8");
  };
  return state;
}

async function main() {
  if (!existsSync(DAEMON) || !existsSync(RELAY)) {
    console.error(`missing binaries; run \`cargo build\` first\n  ${DAEMON}\n  ${RELAY}`);
    process.exit(2);
  }

  // 1. the relay
  startProc("relay", RELAY, {
    ASM_RELAY_BIND: `127.0.0.1:${RELAY_PORT}`,
    ASM_RELAY_KEYS: KEY,
    ASM_RELAY_LOG: "info",
    ASM_RUNTIME_DIR: join(tmp, "relay-run"),
  });
  await waitRelayUp(10000);
  check("relay up", true, RELAY_HTTP);

  // 2. a daemon that registers OUTBOUND to the relay (native backend is fine)
  startProc("daemon", DAEMON, {
    ASM_BIND: `127.0.0.1:${DAEMON_PORT}`,
    ASM_DATA_DIR: dataDir,
    ASM_RUNTIME_DIR: runDir,
    ASM_RELAY_URL: RELAY_WS,
    ASM_RELAY_KEY: KEY,
    ASM_NODE_LABEL: "nat-host",
    ASM_LOG: "info,asm_daemon=debug",
  });
  await waitDaemonHealth(10000);

  // 3. discover it through the relay
  const node = await waitNodeOnline(10000);
  nodeId = node.node_id;
  check("daemon registered + online via relay", !!nodeId && node.online, `${node.label} ${String(nodeId).slice(0, 8)}`);
  check("relay reports the node label", node.label === "nat-host", node.label);

  // 4. enrollment token is fetched out-of-band from the daemon host (loopback)
  const et = await (await fetch(`${DAEMON_HTTP}/api/auth/enrollment-token`)).json();
  check("enrollment token available at the host (loopback)", !!et.enrollment_token);

  // 5. enroll a device THROUGH the relay (enroll is public: relay key only)
  const enrollRes = await relay(`/n/${nodeId}/api/auth/enroll`, {
    method: "POST",
    body: { enrollment_token: et.enrollment_token, device_name: "client-A" },
  });
  const enroll = await enrollRes.json();
  const token = enroll.device_token;
  check("enrolled a device THROUGH the relay", enrollRes.status === 200 && !!token, `status=${enrollRes.status}`);

  // 6. SECURITY: a relayed request with NO token is rejected, even though it
  //    reaches the daemon over a genuinely-loopback tunnel socket.
  const noTok = await relay(`/n/${nodeId}/api/sessions`);
  check("relayed request without a token is 401 (loopback-trust regression)", noTok.status === 401, `status=${noTok.status}`);

  // 7. SECURITY: the enrollment token is never retrievable through the relay,
  //    even with a valid device token.
  const etThroughRelay = await relay(`/n/${nodeId}/api/auth/enrollment-token`, { token });
  check("enrollment-token through the relay is 403", etThroughRelay.status === 403, `status=${etThroughRelay.status}`);

  // 8. create a session THROUGH the relay
  const createRes = await relay(`/n/${nodeId}/api/sessions`, {
    method: "POST",
    token,
    body: { agent_plugin_id: "shell", cwd: tmp },
  });
  const { session } = await createRes.json();
  check("session created through the relay (running)", session && session.status === "running", session && String(session.id).slice(0, 8));
  const sid = session.id;

  // 9. attach the terminal WS through the relay and echo a marker
  const marker = "RELAY-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  const c = wsCollectRelay(sid, token);
  await new Promise((res, rej) => {
    c.ws.onopen = res;
    c.ws.onerror = () => rej(new Error("relayed WS failed to open"));
  });
  await sleep(500);
  c.ws.send(JSON.stringify({ t: "i", d: `echo ${marker}\r` }));
  await sleep(900);
  c.ws.close();
  check("terminal WS through the relay shows the marker", c.buf.includes(marker), marker);

  // 10. stop the session through the relay
  const stopRes = await relay(`/n/${nodeId}/api/sessions/${sid}/stop`, { method: "POST", token });
  check("session stopped through the relay", stopRes.ok, `status=${stopRes.status}`);

  // 11. restart the relay; the daemon must re-register and the loop must work
  stop("relay");
  await waitExit("relay", 5000);
  startProc("relay", RELAY, {
    ASM_RELAY_BIND: `127.0.0.1:${RELAY_PORT}`,
    ASM_RELAY_KEYS: KEY,
    ASM_RELAY_LOG: "info",
    ASM_RUNTIME_DIR: join(tmp, "relay-run"),
  });
  await waitRelayUp(10000);
  await waitNodeOnline(15000); // agent reconnects with backoff
  const afterRestart = await relay(`/n/${nodeId}/api/sessions`, { token });
  check("daemon re-registered after relay restart; API works again", afterRestart.ok, `status=${afterRestart.status}`);

  console.log(
    failures === 0
      ? "\nALL PASS — a NAT'd daemon is fully controllable through the relay"
      : `\n${failures} FAILURE(S)`,
  );
}

main()
  .then(() => {
    cleanup();
    process.exit(failures === 0 ? 0 : 1);
  })
  .catch((e) => {
    console.error(e);
    cleanup();
    process.exit(2);
  });
