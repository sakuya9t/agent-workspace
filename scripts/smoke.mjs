// End-to-end smoke test for the daemon.
//
//   node scripts/smoke.mjs [baseHostPort] [cwd]
//
// Exercises the core product loop plus Git change tracking:
//   create -> attach -> run command -> disconnect -> reconnect (snapshot resume)
//   -> scm status -> stop -> summary.
//
// Requires a running daemon (see README) and Node 18+ (global WebSocket).

const base = process.argv[2] ?? "127.0.0.1:4600";
const cwd = process.argv[3] ?? process.cwd();
const http = `http://${base}`;

let failures = 0;
function check(name, cond, extra) {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`);
  if (!ok) failures++;
  return ok;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function j(path, init) {
  const res = await fetch(http + path, {
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
      typeof ev.data === "string"
        ? ev.data
        : Buffer.from(ev.data).toString("utf8");
  };
  return state;
}

async function main() {
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

  // stop + summary
  const stopped = await j(`/api/sessions/${session.id}/stop`, { method: "POST" });
  check("session stopped", stopped.session.status === "stopped");
  await sleep(300);
  const { summary } = await j(`/api/sessions/${session.id}/summary`);
  check("summary written", !!summary && summary.session_id === session.id, summary?.exit_status);

  console.log(failures === 0 ? "\nALL PASS" : `\n${failures} FAILURE(S)`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
