// End-to-end smoke test for the daemon.
//
//   node scripts/smoke.mjs                 # sandboxed: spawns its own daemon (default)
//   node scripts/smoke.mjs 127.0.0.1:4600  # ATTENDED: smoke an already-running daemon
//
// Exercises the core product loop plus Git change tracking:
//   create -> attach -> run command -> disconnect -> reconnect (snapshot resume)
//   -> scm status -> stop -> summary.
//
// By default this spawns a throwaway daemon on a free port with its own data
// dir, so it can neither see nor mutate the live install. Passing a host:port
// opts into running against a real daemon — every session it creates lands in
// that daemon's data dir, so only do it deliberately.
//
// Requires the built binaries (`cargo build`) and Node 18+ (global WebSocket).

import { execFileSync } from "node:child_process";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const attendedBase = process.argv[2] ?? null;
const { check, report } = checker();

let sb = null;
let base;
let cwd;

async function setup() {
  if (attendedBase) {
    console.log(`!! ATTENDED MODE — running against the live daemon at ${attendedBase}`);
    console.log("!! Sessions created by this run will persist in that daemon's data dir.\n");
    base = attendedBase;
    cwd = process.argv[3] ?? process.cwd();
    return;
  }
  sb = await createSandbox("asm-smoke");
  await sb.startDaemon();
  base = sb.base;
  // A real git repo inside the sandbox, so the scm assertions mean something
  // without reaching into the developer's working tree.
  cwd = sb.cwd;
  execFileSync("git", ["init", "-q"], { cwd });
  execFileSync("git", ["commit", "-q", "--allow-empty", "-m", "init"], {
    cwd,
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: "smoke",
      GIT_AUTHOR_EMAIL: "smoke@test",
      GIT_COMMITTER_NAME: "smoke",
      GIT_COMMITTER_EMAIL: "smoke@test",
    },
  });
  console.log(`sandbox daemon on ${base}  (data: ${sb.dataDir})\n`);
}

const http = () => `http://${base}`;

async function j(path, init) {
  const res = await fetch(http() + path, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) throw new Error(`${path} -> ${res.status} ${await res.text()}`);
  return res.json();
}

function wsCollect(id) {
  const ws = new WebSocket(`ws://${base}/api/sessions/${id}/stream`);
  ws.binaryType = "arraybuffer";
  const state = { buf: "", ws };
  ws.onmessage = (ev) => {
    state.buf +=
      typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString("utf8");
  };
  return state;
}

async function main() {
  await setup();

  const health = await j("/health");
  check("health ok", health.status === "ok", `v${health.version} ${health.platform}`);

  const { plugins } = await j("/api/plugins");
  check("shell plugin available", plugins.find((p) => p.id === "shell")?.available);

  const { session } = await j("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd }),
  });
  check("session created running", session.status === "running", session.id.slice(0, 8));

  // attach + run a marker command
  const marker = "SMOKE-" + Math.floor(health.uptime_ms).toString(36);
  const c1 = wsCollect(session.id);
  await new Promise((res) => (c1.ws.onopen = res));
  await sleep(500);
  c1.ws.send(JSON.stringify({ t: "i", d: `echo ${marker}\r` }));
  await sleep(700);
  c1.ws.close();
  check("live output shows command", c1.buf.includes(marker));

  // reconnect: snapshot should reflect prior output
  const c2 = wsCollect(session.id);
  await new Promise((res) => (c2.ws.onopen = res));
  await sleep(600);
  c2.ws.close();
  check("reconnect snapshot resumes", c2.buf.includes(marker));

  // scm status
  const { status: scm } = await j(`/api/sessions/${session.id}/scm/status`);
  check("scm status reachable", typeof scm.is_repo === "boolean", `repo=${scm.is_repo}`);
  if (!attendedBase) check("scm sees the sandbox repo", scm.is_repo === true);

  // stop + summary
  const stopped = await j(`/api/sessions/${session.id}/stop`, { method: "POST" });
  check("session stopped", stopped.session.status === "stopped");
  await sleep(300);
  const { summary } = await j(`/api/sessions/${session.id}/summary`);
  check("summary written", !!summary && summary.session_id === session.id, summary?.exit_status);

  return report("core loop healthy");
}

main()
  .then((pass) => {
    sb?.cleanup();
    process.exit(pass ? 0 : 1);
  })
  .catch((e) => {
    console.error(e);
    sb?.cleanup();
    process.exit(2);
  });
