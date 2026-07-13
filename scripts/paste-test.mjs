// End-to-end test for file attachment / image paste (POST /api/sessions/:id/paste).
//
//   node scripts/paste-test.mjs                 # sandboxed: spawns its own daemon (default)
//   node scripts/paste-test.mjs 127.0.0.1:4600  # ATTENDED: against a running daemon
//
// Proves the ASM plumbing behind the "attach a file to a session" feature: the
// client uploads raw bytes of ANY type, the daemon stores them under the
// session's working directory, and the returned path is reachable from the
// session's own shell (the same path the client injects into the prompt, which
// Claude Code / Codex then read — that agent step is covered separately by the
// CLI probe in the design doc).
//
// The type allowlist is gone (a PDF or a zip is as useful to an agent as a
// screenshot), so what's under test now is: any bytes are stored; the stored
// name keeps the client's stem + extension; a hostile name can't escape the
// paste dir; and size alone (10 MiB) is what bounds the endpoint.
//
// This test WRITES into the session's cwd (.asm/pastes/, .asm/.gitignore), so by
// default it runs against a throwaway daemon in a throwaway cwd. It used to
// default to the live daemon with cwd=process.cwd(), which littered the repo.
//
// Requires the built binaries (`cargo build`) and Node 18+.

import fs from "node:fs";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const attendedBase = process.argv[2] ?? null;
const { check, report } = checker();

let sb = null;
let base;
let cwd;

async function setup() {
  if (attendedBase) {
    console.log(`!! ATTENDED MODE — live daemon ${attendedBase}; this test writes into the session cwd.\n`);
    base = attendedBase;
    cwd = process.argv[3] ?? process.cwd();
    return;
  }
  sb = await createSandbox("asm-paste");
  await sb.startDaemon();
  base = sb.base;
  cwd = sb.cwd;
  console.log(`sandbox daemon on ${base}  (cwd: ${cwd})\n`);
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

// A valid 1x1 PNG (starts with the PNG magic, so the daemon's sniff accepts it).
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

// A minimal but real PDF and ZIP — the two formats the feature was widened for.
const PDF = Buffer.from("%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n");
const ZIP = Buffer.from("504b0304140000000000000000000000000000000000000000", "hex");

async function post(id, body, contentType, name) {
  const q = name === undefined ? "" : `?name=${encodeURIComponent(name)}`;
  return fetch(`${http()}/api/sessions/${id}/paste${q}`, {
    method: "POST",
    headers: { "content-type": contentType },
    body,
  });
}

async function main() {
  await setup();

  const health = await j("/health");
  check("health ok", health.status === "ok", `v${health.version}`);

  const { session } = await j("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd }),
  });
  check("shell session created", session.status === "running", session.id.slice(0, 8));
  const id = session.id;

  // --- happy path: a clipboard image, which arrives with NO filename ---
  // The extension has to come from the magic bytes (the sniff fallback).
  const res = await post(id, PNG, "image/png");
  check("paste PNG (unnamed) -> 200", res.status === 200, String(res.status));
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

  // --- non-image attachments: the point of the widening ---
  for (const [label, bytes, name, ctype] of [
    ["PDF", PDF, "spec.pdf", "application/pdf"],
    ["ZIP", ZIP, "bundle.zip", "application/zip"],
    ["text", Buffer.from("not an image at all"), "notes.txt", "text/plain"],
  ]) {
    const r = await post(id, bytes, ctype, name);
    check(`attach ${label} -> 200`, r.status === 200, String(r.status));
    if (!r.ok) continue;
    const o = await r.json();
    const ext = name.split(".").pop();
    const stem = name.split(".")[0];
    check(
      `${label} keeps its stem and .${ext}`,
      o.filename.startsWith(`${stem}-`) && o.filename.endsWith(`.${ext}`),
      o.filename,
    );
    check(
      `${label} stored with the right bytes`,
      fs.existsSync(o.path) && fs.readFileSync(o.path).equals(bytes),
      o.path,
    );
  }

  // --- a hostile filename cannot escape the paste dir ---
  const eviI = await post(id, PDF, "application/pdf", "../../../../tmp/evil.pdf");
  check("traversal name -> 200 (sanitised, not rejected)", eviI.status === 200, String(eviI.status));
  if (eviI.ok) {
    const o = await eviI.json();
    check(
      "traversal name collapses into .asm/pastes",
      o.relative_path.startsWith(".asm/pastes/evil-") &&
        !o.relative_path.includes("..") &&
        o.path.startsWith(`${cwd}/.asm/pastes/`),
      o.relative_path,
    );
  }

  // --- size is now the only bound ---
  const big = Buffer.alloc(11 * 1024 * 1024, 0x41);
  const oversize = await post(id, big, "application/octet-stream", "huge.bin");
  check("oversize (>10 MiB) -> 413", oversize.status === 413, String(oversize.status));

  const justUnder = Buffer.alloc(9 * 1024 * 1024, 0x41);
  const ok = await post(id, justUnder, "application/octet-stream", "big.bin");
  check("9 MiB -> 200", ok.status === 200, String(ok.status));

  const empty = await post(id, Buffer.alloc(0), "image/png");
  check("empty body -> 400", empty.status === 400, String(empty.status));

  // cleanup
  try {
    await j(`/api/sessions/${id}/stop`, { method: "POST" });
  } catch {
    /* ignore */
  }

  return report("attachments of any type round-trip into the session cwd");
}

main()
  .then((pass) => {
    sb?.cleanup();
    process.exit(pass ? 0 : 1);
  })
  .catch((e) => {
    console.error(e);
    sb?.cleanup();
    process.exit(1);
  });
