// Gateway end-to-end test — the R4 proof that an *egress-less* downstream (a
// host that cannot reach the relay at all) is fully controllable through a
// gateway daemon that bridges it (docs/connectivity-execution-plan.md, R4).
//
//   node scripts/gateway-test.mjs
//
// Topology, all on loopback (distinct 127.0.0.x addresses emulate distinct
// hosts, the established technique):
//   relay        127.0.0.1:<relay>     access key only
//   gateway  C   127.0.0.2:<c>         registers OUTBOUND to the relay, and
//                                      bridges D via ASM_RELAY_DOWNSTREAMS
//   downstream D 127.0.0.3:<d>         NEVER registers to the relay; reachable
//                                      only by C probing/dialing it inward
//
// The client (this script) only ever talks to the relay. It proves:
//   1. C registers + is online; D is *discovered* by C's /health probe and
//      surfaces in /nodes as a leaf with `via: C`.
//   2. The full session loop runs against D through /n/<D_id> exactly as for a
//      direct node — create, WS attach + marker echo, stop — depth invisible.
//   3. C and D are genuinely distinct daemons: a session created on D does not
//      appear on C (both driven through the same relay).
//   4. The relay key still gates downstream routing (no key ⇒ 401).
//   5. Stopping D flips it to `downstream_unreachable` (offline in /nodes, 502
//      on proxy) while C stays online — then C keeps advertising D's identity.
//
// SECURITY NOTE (loopback caveat). In production the gateway→downstream hop is a
// real network hop, so D sees C's non-loopback address and enforces its device
// token on relayed traffic (the R2 loopback-trust invariant holds by topology).
// This test emulates C and D on 127.0.0.x, so D sees a *loopback* peer and
// grants loopback trust — token enforcement across the gateway therefore cannot
// be reproduced here and is intentionally NOT asserted. The relay-key gate
// (check 4) is the layer this harness can prove. See security-followups.md.
//
// Requires the built binaries (`cargo build`) and Node 18+ (global WebSocket).

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync, openSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const DAEMON = join(ROOT, "target", "debug", "asm-daemon");
const RELAY = join(ROOT, "target", "debug", "asm-relay");

const KEY = "gateway-test-key";
const RELAY_PORT = 5100 + (process.pid % 100);
const C_PORT = 5200 + (process.pid % 100);
const D_PORT = 5300 + (process.pid % 100);
const RELAY_HTTP = `http://127.0.0.1:${RELAY_PORT}`;
const RELAY_WS = `ws://127.0.0.1:${RELAY_PORT}`;
const C_HTTP = `http://127.0.0.2:${C_PORT}`;
const D_HTTP = `http://127.0.0.3:${D_PORT}`;
const PROBE_MS = 700;

let failures = 0;
function check(name, cond, extra) {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra !== undefined ? "  " + extra : ""}`);
  if (!ok) failures++;
  return ok;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const tmp = mkdtempSync(join(tmpdir(), "asm-gateway-"));
const procs = [];

function startProc(name, bin, env) {
  const log = openSync(join(tmp, `${name}.log`), "a");
  const child = spawn(bin, [], {
    env: { ...process.env, ...env },
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

// A relay-routed fetch to node `id`: appends the relay key, optionally a bearer.
async function relay(id, path, { token, method, body, noKey } = {}) {
  const q = noKey ? "" : `${path.includes("?") ? "&" : "?"}relay_key=${KEY}`;
  const url = `${RELAY_HTTP}/n/${id}${path}${q}`;
  const headers = {};
  if (body) headers["content-type"] = "application/json";
  if (token) headers["authorization"] = `Bearer ${token}`;
  return fetch(url, { method, headers, body: body ? JSON.stringify(body) : undefined });
}

async function waitHttp(url, timeoutMs, opts) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url, opts);
      if (res.ok) return res;
    } catch {
      /* not up */
    }
    await sleep(120);
  }
  throw new Error(`did not come up: ${url}`);
}

async function nodes() {
  const res = await fetch(`${RELAY_HTTP}/nodes?relay_key=${KEY}`);
  return res.ok ? (await res.json()).nodes : [];
}

// Poll /nodes until `pred(nodes)` is truthy; returns the matching nodes list.
async function waitNodes(pred, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let last = [];
  while (Date.now() < deadline) {
    try {
      last = await nodes();
      if (pred(last)) return last;
    } catch {
      /* retry */
    }
    await sleep(120);
  }
  throw new Error(`condition never held (${label}); last nodes=${JSON.stringify(last)}`);
}

// Enroll a device with node `id` through the relay. The enrollment token is
// fetched out-of-band from that daemon's own loopback (as a real operator would
// on the host), then redeemed through /n/<id>/api/auth/enroll (public endpoint).
async function enroll(id, hostHttp, deviceName) {
  const et = await (await fetch(`${hostHttp}/api/auth/enrollment-token`)).json();
  const res = await relay(id, "/api/auth/enroll", {
    method: "POST",
    body: { enrollment_token: et.enrollment_token, device_name: deviceName },
  });
  const j = await res.json();
  return { status: res.status, token: j.device_token };
}

function wsCollect(id, sid, token) {
  const url = `${RELAY_WS}/n/${id}/api/sessions/${sid}/stream?access_token=${encodeURIComponent(token)}&relay_key=${KEY}`;
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

  // --- relay ---
  startProc("relay", RELAY, {
    ASM_RELAY_BIND: `127.0.0.1:${RELAY_PORT}`,
    ASM_RELAY_KEYS: KEY,
    ASM_RELAY_LOG: "info",
  });
  await waitHttp(`${RELAY_HTTP}/nodes?relay_key=${KEY}`, 10000);
  check("relay up", true, RELAY_HTTP);

  // --- downstream D (started first so C's first probe already finds it) ---
  startProc("downstream", DAEMON, {
    ASM_BIND: `127.0.0.3:${D_PORT}`,
    ASM_DATA_DIR: join(tmp, "d-data"),
    ASM_RUNTIME_DIR: join(tmp, "d-run"),
    ASM_NODE_LABEL: "downstream-D",
    ASM_LOG: "info",
  });
  await waitHttp(`${D_HTTP}/health`, 10000);
  const dId = (await (await fetch(`${D_HTTP}/health`)).json()).node_id;

  // --- gateway C: registers outbound + bridges D ---
  startProc("gateway", DAEMON, {
    ASM_BIND: `127.0.0.2:${C_PORT}`,
    ASM_DATA_DIR: join(tmp, "c-data"),
    ASM_RUNTIME_DIR: join(tmp, "c-run"),
    ASM_RELAY_URL: RELAY_WS,
    ASM_RELAY_KEY: KEY,
    ASM_NODE_LABEL: "gateway-C",
    ASM_RELAY_DOWNSTREAMS: `127.0.0.3:${D_PORT}`,
    ASM_RELAY_PROBE_INTERVAL_MS: String(PROBE_MS),
    ASM_LOG: "info,asm_daemon=debug",
  });
  await waitHttp(`${C_HTTP}/health`, 10000);
  const cId = (await (await fetch(`${C_HTTP}/health`)).json()).node_id;

  // (1) C online as a gateway; D discovered with via: C.
  const ns = await waitNodes(
    (n) => n.find((x) => x.node_id === cId && x.online) && n.find((x) => x.node_id === dId && x.online),
    10000,
    "C online + D discovered",
  );
  const cEntry = ns.find((x) => x.node_id === cId);
  const dEntry = ns.find((x) => x.node_id === dId);
  check("gateway C registered + online", cEntry?.online && cEntry.kind === "gateway", `${cEntry?.label} kind=${cEntry?.kind}`);
  check("downstream D discovered via C's /health probe", !!dEntry, dId?.slice(0, 8));
  check("D surfaces with via: C and kind leaf", dEntry?.via === cId && dEntry.kind === "leaf", `via=${String(dEntry?.via).slice(0, 8)}`);
  check("D advertises its own label", dEntry?.label === "downstream-D", dEntry?.label);
  check("D is a distinct node_id from C", dId && cId && dId !== cId);

  // Enroll a device with EACH daemon through the same relay.
  const eD = await enroll(dId, D_HTTP, "client-for-D");
  check("enrolled a device with D THROUGH the gateway", eD.status === 200 && !!eD.token, `status=${eD.status}`);
  const eC = await enroll(cId, C_HTTP, "client-for-C");
  check("enrolled a device with C THROUGH the relay", eC.status === 200 && !!eC.token, `status=${eC.status}`);

  // (2) Full session loop against D through /n/<D_id> — identical to a direct node.
  const createRes = await relay(dId, "/api/sessions", {
    method: "POST",
    token: eD.token,
    body: { agent_plugin_id: "shell", cwd: tmp },
  });
  const { session } = await createRes.json();
  check("session created on D through the gateway (running)", session?.status === "running", session && String(session.id).slice(0, 8));
  const sid = session.id;

  const marker = "GW-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  const c = wsCollect(dId, sid, eD.token);
  await new Promise((res, rej) => {
    c.ws.onopen = res;
    c.ws.onerror = () => rej(new Error("relayed WS to D failed to open"));
  });
  await sleep(500);
  c.ws.send(JSON.stringify({ t: "i", d: `echo ${marker}\r` }));
  await sleep(900);
  c.ws.close();
  check("terminal WS to D through the gateway shows the marker", c.buf.includes(marker), marker);

  // (3) Isolation: the session lives on D, not on C.
  const dList = await (await relay(dId, "/api/sessions", { token: eD.token })).json();
  const cList = await (await relay(cId, "/api/sessions", { token: eC.token })).json();
  const onD = (dList.sessions || []).some((s) => s.id === sid);
  const onC = (cList.sessions || []).some((s) => s.id === sid);
  check("D's session is listed on D but NOT on C (distinct daemons)", onD && !onC, `onD=${onD} onC=${onC}`);

  // (4) The relay key gates downstream routing too.
  const noKey = await relay(dId, "/api/sessions", { token: eD.token, noKey: true });
  check("routing to D without the relay key is 401", noKey.status === 401, `status=${noKey.status}`);

  // stop the session (best effort) before tearing D down.
  await relay(dId, `/api/sessions/${sid}/stop`, { method: "POST", token: eD.token });

  // (5) Stop D; C's probe must flip it to unreachable while C stays online.
  stop("downstream");
  await waitExit("downstream", 5000);
  const afterDown = await waitNodes(
    (n) => {
      const d = n.find((x) => x.node_id === dId);
      const cc = n.find((x) => x.node_id === cId);
      return d && !d.online && cc && cc.online;
    },
    10000,
    "D offline + C online",
  );
  const dGone = afterDown.find((x) => x.node_id === dId);
  check("D flipped to offline (downstream_unreachable) while listed", dGone && !dGone.online, `online=${dGone?.online}`);
  check("C stayed online through D's outage", afterDown.find((x) => x.node_id === cId)?.online === true);

  // A proxied request to the now-unreachable D fails fast with the frozen code.
  const proxied = await relay(dId, "/api/sessions", { token: eD.token });
  const perr = proxied.ok ? null : (await proxied.json()).error;
  check("proxy to a downed downstream is 502 downstream_unreachable", proxied.status === 502 && perr === "downstream_unreachable", `status=${proxied.status} error=${perr}`);

  console.log(
    failures === 0
      ? "\nALL PASS — an egress-less downstream is fully controllable through the gateway"
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
