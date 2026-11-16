// TERM-HISTORY proof: a terminal window can never be handed (or keep) more
// history than this browser's limit allows, "Clear history" really clears it
// (across re-attaches), and "Save conversation" still exports EVERYTHING —
// cleared and trimmed output included. See docs/terminal-history-limit.md.
//
//   cargo build -p asm-daemon && (cd client && npm run build)
//   node scripts/term-history-test.mjs
//
// Sandboxed (scripts/lib/testenv.mjs): its own daemon, data dir, port and
// Chrome. Two parts:
//
//   A. Daemon, over the raw stream socket — no browser needed:
//      1. a shell prints `hist-001…300`, then ~330 KB of `bulk-…` lines
//      2. a fresh attach replays all of it (default budget = the whole ring)
//      3. `?replay=65536` replays only the newest 64 KB — no `hist-001`
//      4. POST clear-history; a fresh attach is just the current screen, and
//         output after the clear still streams
//      5. the transcript export still has `hist-001` and `bulk-0001`
//      6. once ended, the history frame is capped too and opens with a
//         "not shown here" marker; the export is still complete; clear → 409
//      7. SCM status lists at most 200 files, tracked edits first, with the
//         real count in `changed_files_total`
//   B. Browser, driving the real bundle:
//      8. the 🧹 button shrinks the terminal's scrollback to one screen and
//         says so; it survives a page reload (the daemon forgot it too)
//      9. Settings → This browser → Terminal history = 250,000 is saved and caps
//         the scrollback to ~250,000 characters' worth of lines on the spot
//     10. in the alternate screen the button is dimmed and explains itself
//     11. the Details panel shows the "N more changed files" footer

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-termhist");
// The raw-replay preamble (backend/mod.rs RAW_REPLAY_PREAMBLE).
const PREAMBLE = "\x1b[?1049l\x1b[r\x1b[H\x1b[2J\x1b[3J\x1b[m";
const DOTS = ".".repeat(100);

/**
 * Attach to a session's stream. `first` resolves with the first frame (the
 * attach snapshot, or an ended session's history frame); `text` accumulates
 * everything. A new live attach supersedes the previous one, so each step opens
 * its own and drives through it.
 */
function attach(id, query = "") {
  const ws = new WebSocket(`ws://${sb.base}/api/sessions/${id}/stream${query}`);
  ws.binaryType = "arraybuffer";
  const st = { ws, text: "" };
  st.first = new Promise((resolve, reject) => {
    let got = false;
    ws.onmessage = (ev) => {
      const buf =
        typeof ev.data === "string" ? Buffer.from(ev.data) : Buffer.from(ev.data);
      st.text += buf.toString("utf8");
      if (!got) {
        got = true;
        resolve(buf);
      }
    };
    ws.onerror = () => reject(new Error("stream socket error"));
  });
  st.open = new Promise((r) => (ws.onopen = r));
  st.send = (d) => ws.send(JSON.stringify({ t: "i", d }));
  st.until = async (needle, ms = 15000) => {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      if (st.text.includes(needle)) return true;
      await sleep(100);
    }
    return false;
  };
  st.close = () => {
    try {
      ws.close();
    } catch {
      /* gone */
    }
  };
  return st;
}

async function firstFrame(id, query = "") {
  const a = attach(id, query);
  const buf = await a.first;
  a.close();
  return buf;
}

const has = (buf, s) => buf.toString("utf8").includes(s);

async function newShell() {
  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  return session.id;
}

// A git cwd with one tracked edit and 250 untracked outputs (step 7 / 11).
function makeRepo() {
  const git = (...args) =>
    execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", ...args], {
      cwd: sb.cwd,
      stdio: "ignore",
    });
  git("init", "-q");
  writeFileSync(join(sb.cwd, "tracked.txt"), "one\n");
  git("add", "tracked.txt");
  git("commit", "-q", "-m", "init");
  writeFileSync(join(sb.cwd, "tracked.txt"), "two\n");
  mkdirSync(join(sb.cwd, "out"));
  for (let i = 0; i < 250; i++) writeFileSync(join(sb.cwd, "out", `run-${i}.json`), "{}\n");
}

async function partA() {
  const id = await newShell();

  // 1. history to replay.
  let a = attach(id);
  await a.open;
  a.ws.send(JSON.stringify({ t: "r", rows: 24, cols: 80 }));
  await sleep(300);
  a.send("seq -f 'hist-%03g' 1 300\r");
  check("shell printed hist-300", await a.until("hist-300"));
  a.send(`seq -f 'bulk-%04g ${DOTS}' 1 3000\r`);
  check("shell printed bulk-3000", await a.until("bulk-3000 "));
  await sleep(500);
  a.close();

  // 2. default budget: the whole ring.
  let snap = await firstFrame(id);
  check("default attach replays the oldest line", has(snap, "hist-001"), `${snap.length} B`);

  // 3. a client budget caps the live replay to the newest output.
  snap = await firstFrame(id, "?replay=65536");
  check(
    "?replay=65536 attach is capped",
    snap.length <= 65536 + PREAMBLE.length,
    `${snap.length} B`,
  );
  check("capped attach starts from the raw-replay preamble", snap.toString().startsWith(PREAMBLE));
  check("capped attach keeps the newest output", has(snap, "bulk-3000"));
  check("capped attach drops the oldest output", !has(snap, "hist-001"));

  // 4. clear: a fresh attach is just the current screen.
  const cleared = await sb.api(`/api/sessions/${id}/clear-history`, { method: "POST" });
  check("clear-history reports what it dropped", cleared.dropped_bytes > 300_000, `${cleared.dropped_bytes} B`);
  snap = await firstFrame(id);
  check("after clear, the attach is one screen", snap.length < 16_384, `${snap.length} B`);
  check("after clear, old history is gone", !has(snap, "hist-001") && !has(snap, "bulk-0001"));
  check("after clear, the current screen survives", has(snap, "bulk-3000"));

  a = attach(id);
  await a.open;
  await a.first;
  a.send("echo after-$((40+2))\r");
  check("output after the clear streams live", await a.until("after-42"));
  await sleep(300);
  a.close();
  snap = await firstFrame(id);
  check("output after the clear is replayed on attach", has(snap, "after-42"));
  check("…and the cleared history stays cleared", !has(snap, "hist-001"));

  // 5. the export is untouched by the clear.
  const exportText = async () => {
    const r = await fetch(`${sb.http}/api/sessions/${id}/transcript`);
    return r.ok ? await r.text() : "";
  };
  let exp = await exportText();
  check(
    "export still has the cleared history",
    exp.includes("hist-001") && exp.includes("bulk-0001") && exp.includes("after-42"),
    `${exp.length} B`,
  );

  // 6. ended session: the history frame is capped and says so.
  await sb.api(`/api/sessions/${id}/stop?force=true`, { method: "POST" });
  for (let i = 0; i < 50; i++) {
    const { session } = await sb.api(`/api/sessions/${id}`);
    if (session.status !== "running") break;
    await sleep(200);
  }
  snap = await firstFrame(id);
  check(
    "ended, default budget: the whole (small) record replays unmarked",
    has(snap, "hist-001") && !has(snap, "not shown here"),
    `${snap.length} B`,
  );
  snap = await firstFrame(id, "?replay=65536");
  check(
    "ended, ?replay=65536: frame is capped",
    snap.length <= 65536 + PREAMBLE.length + 200,
    `${snap.length} B`,
  );
  check("ended, capped: opens with the preamble", snap.toString().startsWith(PREAMBLE));
  check("ended, capped: says earlier output is not shown", has(snap, "not shown here"));
  check("ended, capped: newest output kept, oldest dropped", has(snap, "after-42") && !has(snap, "hist-001"));
  exp = await exportText();
  check("ended: export is still complete", exp.includes("hist-001") && exp.includes("bulk-0001"));
  const r = await fetch(`${sb.http}/api/sessions/${id}/clear-history`, { method: "POST" });
  check("clear-history on an ended session is 409", r.status === 409, String(r.status));

  // 7. SCM status cap.
  const { status } = await sb.api(`/api/sessions/${id}/scm/status`);
  check(
    "scm status lists at most 200 files",
    status.changed_files.length === 200,
    String(status.changed_files.length),
  );
  check("scm status reports the real count", status.changed_files_total === 251, String(status.changed_files_total));
  check("a tracked edit is never hidden behind untracked files", status.changed_files[0]?.path === "tracked.txt");
}

async function partB() {
  const id = await newShell();
  const chrome = await sb.startChrome();
  const page = await chrome.openPage("about:blank");
  const { S, evalJs, waitFor } = page;
  await S("Network.enable");
  await S("Network.setCacheDisabled", { cacheDisabled: true });
  await S("Emulation.setDeviceMetricsOverride", { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false });
  await S("Page.navigate", { url: `${sb.http}/` });

  // Part A left an ended session in the list; pick the running one.
  const LIVE_ROW = `[...document.querySelectorAll('.session-row')].find(r => /running/i.test(r.querySelector('.status-dot')?.title ?? ''))`;
  check("live session row rendered", await waitFor(`!!${LIVE_ROW}`));
  await evalJs(`${LIVE_ROW}.click()`);
  check("terminal mounted", await waitFor("!!document.querySelector('.xterm-rows')"));
  check("🧹 Clear history button rendered", await waitFor("!!document.querySelector('.term-actions .term-clear')"));

  // Buffer lines = scroll-area height / row height (xterm sizes the scroll area
  // to the whole buffer).
  const lines = () =>
    evalJs(`(() => {
      const area = document.querySelector('.xterm-scroll-area');
      const row = document.querySelector('.xterm-rows > div');
      return area && row ? Math.round(area.getBoundingClientRect().height / row.getBoundingClientRect().height) : -1;
    })()`);
  const termRows = await evalJs("document.querySelectorAll('.xterm-rows > div').length");
  const typeIn = (s) =>
    evalJs(`(() => { const ta = document.querySelector('.xterm-helper-textarea'); ta.focus();
      document.execCommand('insertText', false, ${JSON.stringify(s)}); return true; })()`);
  const enter = () =>
    S("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" }).then(
      () => S("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 }),
    );

  await sleep(800);
  await typeIn("seq -f 'ui-%05g' 1 2000");
  await enter();
  check(
    "terminal filled with 2000 lines",
    await waitFor(`(() => { const a=document.querySelector('.xterm-scroll-area'); const r=document.querySelector('.xterm-rows > div');
      return a && r && a.getBoundingClientRect().height / r.getBoundingClientRect().height > 2000; })()`),
  );
  const before = await lines();

  // 8. clear.
  await evalJs("document.querySelector('.term-actions .term-clear').click()");
  check("clear shows a receipt", await waitFor("!!document.querySelector('.paste-status--ok')"));
  check(
    "clear shrinks the scrollback to about one screen",
    await waitFor(`(() => { const a=document.querySelector('.xterm-scroll-area'); const r=document.querySelector('.xterm-rows > div');
      return a && r && a.getBoundingClientRect().height / r.getBoundingClientRect().height <= ${termRows + 2}; })()`),
    `${before} → ${await lines()} lines (screen ${termRows})`,
  );
  await S("Page.reload", { ignoreCache: true });
  await waitFor(`!!${LIVE_ROW}`);
  await evalJs(`${LIVE_ROW}.click()`);
  await waitFor("!!document.querySelector('.xterm-rows')");
  await sleep(1500);
  check("the clear survives a reload", (await lines()) <= termRows + 2, `${await lines()} lines`);

  // 9. the per-browser limit.
  await evalJs("document.querySelector('.config-btn').click()");
  check("Settings shows the terminal history limit", await waitFor("!!document.querySelector('.config-browser-select')"));
  await evalJs(`(() => { const s = document.querySelector('.config-browser-select');
    s.value = '250000'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
  check(
    "the limit is saved in this browser",
    (await evalJs("localStorage.getItem('asm.terminalHistoryChars')")) === "250000",
  );
  await evalJs("[...document.querySelectorAll('.modal .btn')].find(b => b.textContent.trim() === 'Close')?.click()");
  // The terminal reports its geometry to the daemon on attach.
  const { cols } = (await sb.api(`/api/sessions/${id}`)).session;
  const cap = Math.ceil(250000 / cols);
  await typeIn("seq -f 'cap-%05g' 1 12000");
  await enter();
  await sleep(500);
  await waitFor(`document.querySelector('.xterm-rows').textContent.includes('cap-12000')`, 30000);
  await sleep(800);
  const capped = await lines();
  check(
    "scrollback is capped at ~250,000 characters of lines",
    capped <= cap + termRows + 2 && capped >= cap - 2,
    `${capped} lines, cap ${cap} at ${cols} cols`,
  );

  // 10. alternate screen: the button is dimmed and explains.
  await typeIn("printf '\\033[?1049h'; sleep 30");
  await enter();
  check("button dims in the alternate screen", await waitFor("!!document.querySelector('.term-clear.is-inert')"));
  await evalJs("document.querySelector('.term-clear').click()");
  check(
    "…and a click explains instead of clearing",
    await waitFor("document.querySelector('.paste-status--error')?.textContent.includes('own history')"),
  );

  // 11. the Details panel footer for a capped file list.
  check(
    "Details panel says how many changed files aren't listed",
    await waitFor("document.querySelector('.changed-files-more')?.textContent.includes('51 more changed files')"),
    await evalJs("document.querySelector('.changed-files-more')?.textContent ?? ''"),
  );
  check(
    "Details panel renders only the capped list",
    (await evalJs("document.querySelectorAll('.changed-file').length")) === 200,
  );
}

try {
  makeRepo();
  await sb.startAppDaemon();
  await partA();
  await partB();
} catch (e) {
  check("no exception", false, e?.stack ?? String(e));
} finally {
  sb.cleanup();
}
process.exit(report("terminal history is bounded, clearable, and export stays complete") ? 0 : 1);
