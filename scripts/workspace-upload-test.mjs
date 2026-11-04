// End-to-end test for workspace upload (POST /api/sessions/:id/upload).
//
//   node scripts/workspace-upload-test.mjs                 # sandboxed: spawns its own daemon (default)
//   node scripts/workspace-upload-test.mjs 127.0.0.1:4600  # ATTENDED: against a running daemon
//
// The Details panel's "Upload files" button puts a file *in the workspace*, at
// `uploads/<name>`, so the session's agent can find it by listing a directory
// rather than by being handed a path. That is the difference from `paste`, and
// it is what this test pins down:
//
//   - the stored name is the one the client sent — no uuid, no rewriting, and an
//     extension-less name like `Makefile` survives intact;
//   - a second upload of the same name is refused with 409 rather than
//     overwriting (this directory is inside the user's checkout, so a silent
//     clobber could destroy source), and `force=true` is what replaces it;
//   - a forced replace *unlinks first*, so a symlink planted at that name is
//     replaced rather than written through to wherever it pointed;
//   - a hostile name still collapses into `uploads/`;
//   - the session's own shell reads the file at the returned relative path.
//
// This test WRITES into the session's cwd (uploads/), so by default it runs
// against a throwaway daemon in a throwaway cwd.
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
  sb = await createSandbox("asm-wsupload");
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

async function put(id, body, name, { force = false, contentType = "application/octet-stream" } = {}) {
  const q = `?name=${encodeURIComponent(name)}` + (force ? "&force=true" : "");
  return fetch(`${http()}/api/sessions/${id}/upload${q}`, {
    method: "POST",
    headers: { "content-type": contentType },
    body,
  });
}

const PDF = Buffer.from("%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n");

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

  // --- the name the user picked is the name on disk ---
  const first = await put(id, PDF, "spec.pdf", { contentType: "application/pdf" });
  check("upload spec.pdf -> 200", first.status === 200, String(first.status));
  const out = first.ok ? await first.json() : {};
  check(
    "stored under uploads/ with the exact name (no uuid)",
    out.relative_path === "uploads/spec.pdf" && out.filename === "spec.pdf",
    out.relative_path,
  );
  check(
    "file exists on host with the right bytes",
    out.path && fs.existsSync(out.path) && fs.readFileSync(out.path).equals(PDF),
    out.path,
  );
  check(
    "uploads/ is NOT git-ignored (it is working material, unlike .asm/)",
    !fs.existsSync(`${cwd}/uploads/.gitignore`),
  );

  // An extension-less name is a real name — `Makefile.bin` would break it.
  const mk = await put(id, Buffer.from("all:\n\techo hi\n"), "Makefile");
  check("upload Makefile -> 200", mk.status === 200, String(mk.status));
  if (mk.ok) {
    const o = await mk.json();
    check("extension-less name preserved", o.filename === "Makefile", o.filename);
  }

  // --- the injected path is reachable from the session's own shell ---
  const ws = new WebSocket(`ws://${base}/api/sessions/${id}/stream`);
  ws.binaryType = "arraybuffer";
  let buf = "";
  ws.onmessage = (ev) =>
    (buf += typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString("utf8"));
  await new Promise((r) => (ws.onopen = r));
  await sleep(400);
  ws.send(JSON.stringify({ t: "i", d: `wc -c < 'uploads/spec.pdf'\r` }));
  await sleep(700);
  ws.close();
  check(
    "shell reads the uploaded file at uploads/<name>",
    buf.includes(String(PDF.length)),
    `expected ${PDF.length} bytes in output`,
  );

  // --- a collision is refused, not silently applied ---
  const dupe = await put(id, Buffer.from("REPLACED"), "spec.pdf");
  check("same name again -> 409", dupe.status === 409, String(dupe.status));
  check(
    "the refused upload did not touch the original bytes",
    fs.readFileSync(`${cwd}/uploads/spec.pdf`).equals(PDF),
  );

  const forced = await put(id, Buffer.from("REPLACED"), "spec.pdf", { force: true });
  check("force=true -> 200", forced.status === 200, String(forced.status));
  check(
    "forced upload replaced the contents",
    fs.readFileSync(`${cwd}/uploads/spec.pdf`, "utf8") === "REPLACED",
  );

  // --- a forced replace unlinks rather than writing through a symlink ---
  // An agent running in this session could plant one; following it would let an
  // upload write anywhere on the host the daemon user can reach.
  const outside = `${cwd}/outside-the-workspace.txt`;
  fs.writeFileSync(outside, "ORIGINAL");
  fs.symlinkSync(outside, `${cwd}/uploads/link.txt`);
  const viaLink = await put(id, Buffer.from("THROUGH"), "link.txt");
  check("existing symlink counts as a collision -> 409", viaLink.status === 409, String(viaLink.status));
  const replacedLink = await put(id, Buffer.from("THROUGH"), "link.txt", { force: true });
  check("forced replace over a symlink -> 200", replacedLink.status === 200, String(replacedLink.status));
  check(
    "the symlink target was NOT written through",
    fs.readFileSync(outside, "utf8") === "ORIGINAL",
    fs.readFileSync(outside, "utf8"),
  );
  check(
    "uploads/link.txt is now a regular file with the uploaded bytes",
    !fs.lstatSync(`${cwd}/uploads/link.txt`).isSymbolicLink() &&
      fs.readFileSync(`${cwd}/uploads/link.txt`, "utf8") === "THROUGH",
  );

  // --- a directory in the way is a 400, not a retryable 409 ---
  fs.mkdirSync(`${cwd}/uploads/adir`, { recursive: true });
  const onDir = await put(id, PDF, "adir");
  check("directory in the way -> 400 (no confirm would help)", onDir.status === 400, String(onDir.status));

  // --- a hostile filename cannot escape uploads/ ---
  const evil = await put(id, PDF, "../../../../tmp/evil.pdf");
  check("traversal name -> 200 (sanitised, not rejected)", evil.status === 200, String(evil.status));
  if (evil.ok) {
    const o = await evil.json();
    check(
      "traversal name collapses into uploads/",
      o.relative_path === "uploads/evil.pdf" &&
        !o.relative_path.includes("..") &&
        o.path === `${cwd}/uploads/evil.pdf`,
      o.relative_path,
    );
  }

  // --- the same size bound as paste ---
  const oversize = await put(id, Buffer.alloc(11 * 1024 * 1024, 0x41), "huge.bin");
  check("oversize (>10 MiB) -> 413", oversize.status === 413, String(oversize.status));

  const empty = await put(id, Buffer.alloc(0), "nothing.txt");
  check("empty body -> 400", empty.status === 400, String(empty.status));

  // cleanup
  try {
    await j(`/api/sessions/${id}/stop`, { method: "POST" });
  } catch {
    /* ignore */
  }

  return report("files upload into the workspace at a predictable uploads/<name>");
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
