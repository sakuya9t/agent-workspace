// TERM-SCROLL proof — a normal-buffer agent's (codex-shaped) attach snapshot
// carries its scrollback, so a freshly-attached client can scroll back to the
// start of the conversation. See docs/terminal-scrollback.md.
//
//   node scripts/termscroll-test.mjs
//
// It orchestrates everything itself:
//   1. start a native daemon on a throwaway port + data dir
//   2. drive a shell session into the CODEX SHAPE — a bottom-margin scroll
//      region (ESC[1;21r) with 100 lines scrolled through the top of it. The
//      daemon's vt100 drops that region's scrollback, so the OLD rendered
//      attach repaint carried none of those lines.
//   3. attach fresh and assert the snapshot begins with the raw-replay preamble
//      and CONTAINS the oldest scrolled-off line (`line-001`) — i.e. the history
//      is delivered. No terminal emulator is needed: "does the payload contain
//      line-001" is itself the discriminator (the old path could not).
//   4. drive another session into the ALTERNATE screen and assert its snapshot
//      still uses the rendered repaint (arms ESC[?1049h, no raw-replay preamble)
//      — the claude path must be unchanged.
//
// Requires the built binary (`cargo build -p asm-daemon`) and Node 18+.

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, openSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const DAEMON = join(ROOT, "target", "debug", "asm-daemon");
const PORT = 4680 + (process.pid % 80);
const BASE = `127.0.0.1:${PORT}`;
const HTTP = `http://${BASE}`;
// Raw-replay preamble emitted for a normal-buffer attach (see backend/mod.rs).
const PREAMBLE = "\x1b[?1049l\x1b[r\x1b[H\x1b[2J\x1b[3J\x1b[m";

let failures = 0;
const check = (name, cond, extra) => {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`);
  if (!ok) failures++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const tmp = mkdtempSync(join(tmpdir(), "asm-termscroll-"));
let daemon;

async function waitHealth(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(`${HTTP}/health`)).ok) return;
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

// Send input to a session (with a known geometry) and wait for it to render.
function drive(id, input) {
  return new Promise((resolve) => {
    const ws = new WebSocket(`ws://${BASE}/api/sessions/${id}/stream`);
    ws.binaryType = "arraybuffer";
    ws.onopen = async () => {
      ws.send(JSON.stringify({ t: "r", rows: 24, cols: 80 }));
      await sleep(300);
      ws.send(JSON.stringify({ t: "i", d: input }));
      await sleep(1500);
      ws.close();
      resolve();
    };
  });
}

// Fresh attach; return the concatenated snapshot+tail bytes as a latin1 string
// (byte-faithful, so control sequences and `line-NNN` text both survive).
function attachBytes(id, ms) {
  return new Promise((resolve) => {
    const ws = new WebSocket(`ws://${BASE}/api/sessions/${id}/stream`);
    ws.binaryType = "arraybuffer";
    const parts = [];
    ws.onmessage = (ev) => {
      if (typeof ev.data !== "string") parts.push(Buffer.from(ev.data));
    };
    ws.onopen = () =>
      setTimeout(() => {
        ws.close();
        resolve(Buffer.concat(parts).toString("latin1"));
      }, ms);
  });
}

async function main() {
  daemon = spawn(DAEMON, [], {
    env: {
      ...process.env,
      ASM_BIND: BASE,
      ASM_DATA_DIR: join(tmp, "data"),
      ASM_CONFIG_DIR: join(tmp, "config"),
      ASM_RUNTIME_DIR: join(tmp, "run"),
    },
    stdio: ["ignore", openSync(join(tmp, "daemon.log"), "a"), openSync(join(tmp, "daemon.log"), "a")],
  });
  await waitHealth(15000);

  // --- Normal-buffer / codex shape: scrollback must be delivered on attach ---
  const { session: codex } = await j("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: "/tmp" }),
  });
  const emit =
    `printf '\\033[1;21r'; i=1; while [ $i -le 100 ]; do ` +
    `printf '\\033[21;1Hline-%03d\\n' $i; i=$((i+1)); done; ` +
    `printf '\\033[rDONE-COMPOSER\\n'\r`;
  await drive(codex.id, emit);
  await sleep(300);
  const snap = await attachBytes(codex.id, 800);

  check("codex-shape attach begins with raw-replay preamble", snap.startsWith(PREAMBLE));
  check(
    "oldest scrolled-off line 'line-001' delivered on attach (old path dropped it)",
    snap.includes("line-001"),
  );
  check("mid line 'line-050' delivered", snap.includes("line-050"));
  check("live composer 'DONE-COMPOSER' present", snap.includes("DONE-COMPOSER"));
  await j(`/api/sessions/${codex.id}/stop`, { method: "POST" }).catch(() => {});

  // --- Alternate screen / claude shape: rendered repaint, unchanged ---
  const { session: alt } = await j("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: "/tmp" }),
  });
  await drive(alt.id, `printf '\\033[?1049h\\033[?1006h\\033[HALT-SCREEN-APP'\r`);
  await sleep(300);
  const altSnap = await attachBytes(alt.id, 700);

  check("alt-screen attach arms ESC[?1049h (rendered path)", altSnap.startsWith("\x1b[?1049h"));
  check("alt-screen attach does NOT use the raw-replay preamble", !altSnap.startsWith(PREAMBLE));
  await j(`/api/sessions/${alt.id}/stop`, { method: "POST" }).catch(() => {});

  console.log(failures === 0 ? "\nALL PASS" : `\n${failures} FAILURE(S)`);
}

main()
  .catch((e) => {
    console.error(e);
    failures++;
  })
  .finally(() => {
    if (daemon) daemon.kill("SIGTERM");
    try {
      rmSync(tmp, { recursive: true, force: true });
    } catch {
      /* best effort */
    }
    process.exit(failures === 0 ? 0 : 1);
  });
