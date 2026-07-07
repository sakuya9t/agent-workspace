// End-to-end test for image paste (POST /api/sessions/:id/paste).
//
//   node scripts/paste-test.mjs [baseHostPort] [cwd]
//
// Proves the ASM plumbing behind the "paste a screenshot into a session"
// feature: the client uploads image bytes, the daemon stores them under the
// session's working directory, and the returned path is reachable from the
// session's own shell (the same path the client injects into the prompt, which
// Claude Code / Codex then load as an image — that agent step is covered
// separately by the CLI probe in the design doc).
//
// Requires a running daemon on loopback (loopback is trusted, so no token).
// Node 18+ (global fetch + WebSocket).

import fs from "node:fs";

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

// A valid 1x1 PNG (starts with the PNG magic, so the daemon's sniff accepts it).
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

async function post(id, body, contentType) {
  return fetch(`${http}/api/sessions/${id}/paste`, {
    method: "POST",
    headers: { "content-type": contentType },
    body,
  });
}

async function main() {
  const health = await j("/health");
  check("health ok", health.status === "ok", `v${health.version}`);

  const { session } = await j("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd }),
  });
  check("shell session created", session.status === "running", session.id.slice(0, 8));
  const id = session.id;

  // --- happy path: upload a PNG ---
  const res = await post(id, PNG, "image/png");
  check("paste PNG -> 200", res.status === 200, String(res.status));
  const out = res.ok ? await res.json() : {};
  check("response has absolute path", typeof out.path === "string" && out.path.endsWith(".png"));
  check(
    "response relative_path under .asm/pastes",
    typeof out.relative_path === "string" && out.relative_path.startsWith(".asm/pastes/"),
    out.relative_path,
  );
  check(
    "file exists on host with the right bytes",
    out.path && fs.existsSync(out.path) && fs.statSync(out.path).size === PNG.length,
    out.path,
  );
  check(
    ".asm/.gitignore written as '*'",
    fs.existsSync(`${cwd}/.asm/.gitignore`) &&
      fs.readFileSync(`${cwd}/.asm/.gitignore`, "utf8").trim() === "*",
  );

  // --- the injected path is reachable from the session's own shell ---
  // (the client injects `[pasted image <relative_path>] ` into the prompt;
  // here we prove the shell can read the file at exactly that relative path).
  const ws = new WebSocket(`ws://${base}/api/sessions/${id}/stream`);
  ws.binaryType = "arraybuffer";
  let buf = "";
  ws.onmessage = (ev) =>
    (buf += typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString("utf8"));
  await new Promise((r) => (ws.onopen = r));
  await sleep(400);
  ws.send(JSON.stringify({ t: "i", d: `wc -c < '${out.relative_path}'\r` }));
  await sleep(700);
  ws.close();
  check(
    "shell reads the pasted file at the returned path",
    buf.includes(String(PNG.length)),
    `expected ${PNG.length} bytes in output`,
  );

  // --- negative cases ---
  const txt = await post(id, Buffer.from("not an image at all"), "text/plain");
  check("non-image -> 415", txt.status === 415, String(txt.status));

  const big = Buffer.concat([PNG, Buffer.alloc(6 * 1024 * 1024)]);
  const oversize = await post(id, big, "image/png");
  check("oversize -> 413", oversize.status === 413, String(oversize.status));

  const empty = await post(id, Buffer.alloc(0), "image/png");
  check("empty body -> 400", empty.status === 400, String(empty.status));

  // cleanup
  try {
    await j(`/api/sessions/${id}/stop`, { method: "POST" });
  } catch {
    /* ignore */
  }

  console.log(failures ? `\n${failures} FAILURE(S)` : "\nALL PASS");
  process.exit(failures ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
