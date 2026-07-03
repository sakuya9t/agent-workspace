// Durable-session restart test — the headline proof that sessions survive a
// daemon restart via the out-of-process asmux holder (durable-sessions.md M2+M3).
//
//   node scripts/durable-restart-test.mjs
//
// It orchestrates everything itself:
//   1. start asmux (the holder) on a private socket
//   2. start daemon #1 with ASM_BACKEND=sidecar
//   3. create a shell session, run a marker command, confirm it is running
//   4. SIGTERM daemon #1 (the holder keeps the PTY alive)
//   5. start daemon #2 on the same data dir + holder socket
//   6. assert the session was ADOPTED: still `running`, screen reconstructed
//      (marker present), and still accepting input (a second marker echoes back)
//
// Requires the built binaries (`cargo build`) and Node 18+ (global WebSocket).

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync, openSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const DAEMON = join(ROOT, "target", "debug", "asm-daemon");
const ASMUX = join(ROOT, "target", "debug", "asmux");
const PORT = 4700 + (process.pid % 150);
const BASE = `127.0.0.1:${PORT}`;
const HTTP = `http://${BASE}`;

let failures = 0;
function check(name, cond, extra) {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`);
  if (!ok) failures++;
  return ok;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const tmp = mkdtempSync(join(tmpdir(), "asm-dur-"));
const runDir = join(tmp, "run");
const dataDir = join(tmp, "data");
const sock = join(runDir, "asmux.sock");
const procs = [];

function startProc(name, bin, env) {
  const log = openSync(join(tmp, `${name}.log`), "a");
  const child = spawn(bin, [], {
    env: { ...process.env, ...env },
    stdio: ["ignore", log, log],
    detached: name === "asmux", // holder gets its own group; it must outlive the daemon
  });
  procs.push({ name, child });
  return child;
}

async function waitHealth(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${HTTP}/health`);
      if (res.ok) return await res.json();
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }
  throw new Error("daemon /health did not come up");
}

async function j(path, init) {
  const res = await fetch(HTTP + path, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) throw new Error(`${path} -> ${res.status} ${await res.text()}`);
  return res.json();
}

function wsCollect(id) {
  const ws = new WebSocket(`ws://${BASE}/api/sessions/${id}/stream`);
  ws.binaryType = "arraybuffer";
  const state = { buf: "", ws };
  ws.onmessage = (ev) => {
    state.buf +=
      typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString("utf8");
  };
  return state;
}

function daemonEnv() {
  return {
    ASM_BIND: BASE,
    ASM_DATA_DIR: dataDir,
    ASM_RUNTIME_DIR: runDir,
    ASM_BACKEND: "sidecar",
    ASM_ASMUX_AUTOSPAWN: "0", // we manage the holder ourselves for determinism
    ASM_LOG: "info,asm_daemon=debug",
  };
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

async function main() {
  if (!existsSync(DAEMON) || !existsSync(ASMUX)) {
    console.error(`missing binaries; run \`cargo build\` first\n  ${DAEMON}\n  ${ASMUX}`);
    process.exit(2);
  }

  // 1. the holder
  startProc("asmux", ASMUX, { ASM_RUNTIME_DIR: runDir, ASM_LOG: "info" });
  for (let i = 0; i < 40 && !existsSync(sock); i++) await sleep(100);
  check("holder (asmux) socket up", existsSync(sock), sock);

  // 2. daemon #1
  startProc("daemon1", DAEMON, daemonEnv());
  const health = await waitHealth(10000);
  check("daemon1 up on sidecar backend", health.backend === "asmux-sidecar", health.backend);

  // 3. create a shell session + run a marker
  const { session } = await j("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: tmp }),
  });
  check("session created running", session.status === "running", session.id.slice(0, 8));
  const id = session.id;

  const marker = "DURABLE-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  const c1 = wsCollect(id);
  await new Promise((res) => (c1.ws.onopen = res));
  await sleep(500);
  c1.ws.send(JSON.stringify({ t: "i", d: `echo ${marker}\r` }));
  await sleep(800);
  c1.ws.close();
  check("live output shows marker (pre-restart)", c1.buf.includes(marker));

  // 4. kill daemon #1 — the holder must keep the PTY alive
  stop("daemon1");
  await waitExit("daemon1", 5000);
  check("daemon1 exited", true);
  check("holder still alive after daemon exit", existsSync(sock));

  // 5. daemon #2 on the same data dir + holder socket
  startProc("daemon2", DAEMON, daemonEnv());
  await waitHealth(10000);

  // 6. the durability assertions
  const after = await j(`/api/sessions/${id}`);
  check(
    "session ADOPTED as running after restart",
    after.session.status === "running",
    `status=${after.session.status}`,
  );

  const c2 = wsCollect(id);
  await new Promise((res) => (c2.ws.onopen = res));
  await sleep(700);
  check("screen reconstructed after restart (marker present)", c2.buf.includes(marker));

  // prove it is genuinely alive, not just a replayed corpse
  const marker2 = "ALIVE-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  c2.ws.send(JSON.stringify({ t: "i", d: `echo ${marker2}\r` }));
  await sleep(900);
  c2.ws.close();
  check("session still accepts input after restart", c2.buf.includes(marker2), marker2);

  // clean stop
  await j(`/api/sessions/${id}/stop`, { method: "POST" }).catch(() => {});

  console.log(failures === 0 ? "\nALL PASS — sessions survive daemon restart" : `\n${failures} FAILURE(S)`);
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
