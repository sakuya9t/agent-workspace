// Headless-Chrome verification of the Git-history "commit the changes" button
// against a real Codex TUI, driving the built client bundle served by the daemon.
//
//   cd client && npm run build
//   node scripts/commit-button-test.mjs
//
// The button injects a prompt and an Enter as two separate writes. The Enter has
// to land *outside* the TUI's Enter-suppression window — Codex keeps a newline
// literal for 120ms after a paste-like burst (PASTE_ENTER_SUPPRESS_WINDOW), so a
// gap inside that window leaves the prompt sitting in the composer with the
// cursor on a fresh line instead of sending it.
//
// What this covers is the *wire shape* the button produces: two input frames,
// the prompt then a bare Enter, spaced widely enough to clear that window. It
// deliberately makes no assertion about the TUI's resulting state — xterm may
// render to canvas, which leaves the screen unreadable from the DOM.
//
// Codex runs through a shim (CODEX_SHIM_DIR) that points it at a dead provider,
// so a submitted prompt fails instantly instead of spending model tokens, and
// that sets check_for_update_on_startup=false.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-commit-btn");

// Codex only treats the Enter as a keypress once its burst window has retired.
const MIN_GAP_MS = 130;

async function main() {
  // The whole point of this test is to press a button that sends a prompt, so an
  // un-shimmed Codex would submit "commit the changes" to a live model — and let
  // it loose on the sandbox repo. Require the shim unless that is asked for.
  const shim = process.env.CODEX_SHIM_DIR;
  if (!shim && !process.env.ALLOW_REAL_CODEX) {
    throw new Error(
      "set CODEX_SHIM_DIR to a dir holding a `codex` shim (dead provider + " +
        "check_for_update_on_startup=false), or ALLOW_REAL_CODEX=1 to drive the real one",
    );
  }
  await sb.startAppDaemon("daemon", shim ? { PATH: `${shim}:${process.env.PATH}` } : {});

  // A repo with a dirty file — the button is disabled when nothing has changed.
  const git = (...args) => execFileSync("git", args, { cwd: sb.cwd, encoding: "utf8" });
  git("init", "-q");
  git("config", "user.email", "test@example.com");
  git("config", "user.name", "test");
  fs.writeFileSync(join(sb.cwd, "a.txt"), "hello\n");
  git("add", "-A");
  git("commit", "-qm", "init");
  fs.writeFileSync(join(sb.cwd, "a.txt"), "hello\nchanged\n");

  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "codex", cwd: sb.cwd }),
  });
  check("codex session created", session.status === "running", session.id.slice(0, 8));

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { evalJs, waitFor } = page;

  // Tap outgoing input frames before the app opens its socket, so the button's
  // writes are observable with their real timing.
  await evalJs(`
    window.__frames = [];
    const send = WebSocket.prototype.send;
    WebSocket.prototype.send = function (data) {
      try {
        const m = JSON.parse(data);
        if (m && m.t === "i") window.__frames.push({ d: m.d, at: performance.now() });
      } catch { /* binary/resize frames are not input */ }
      return send.call(this, data);
    };
    true
  `);

  check("session row rendered", await waitFor("!!document.querySelector('.session-row')"));
  await evalJs("document.querySelector('.session-row').click()");

  // Give Codex time to boot before driving the button. Deliberately a sleep and
  // not a screen check: xterm may render to canvas, so `.xterm-screen` innerText
  // is empty on some renderers and cannot be relied on for readiness.
  //
  // The shim must disable Codex's startup update check — that modal defaults to
  // "Update now", so a prompt injected while it is up confirms it and
  // npm-installs over the host's Codex.
  await sleep(20000);

  const btnSel = "button.icon-btn:has(.action-icon-git-commit)";
  check("commit button rendered", await waitFor(`!!document.querySelector('${btnSel}')`, 30000));
  check(
    "commit button enabled (repo is dirty)",
    await waitFor(`!document.querySelector('${btnSel}').disabled`, 30000),
  );

  await evalJs(`window.__frames = []; document.querySelector('${btnSel}').click(); true`);
  await sleep(1500);

  const frames = JSON.parse(await evalJs("JSON.stringify(window.__frames)"));
  check("button emitted two input frames", frames.length === 2, JSON.stringify(frames.map((f) => f.d)));
  check("first frame is the prompt", frames[0]?.d === "commit the changes");
  check("second frame is a bare Enter", frames[1]?.d === "\r");

  const gap = frames.length === 2 ? Math.round(frames[1].at - frames[0].at) : -1;
  check(
    `Enter clears the TUI suppression window (${gap}ms >= ${MIN_GAP_MS}ms)`,
    gap >= MIN_GAP_MS,
    `gap=${gap}ms`,
  );

  // No assertion on the TUI's own state follows — see the header. Confirming
  // that Codex actually *submits* on this wire shape means reading the pty
  // through a terminal emulator, which is a job for a separate harness.
}

try {
  await main();
} finally {
  await sb.cleanup();
}
report();
