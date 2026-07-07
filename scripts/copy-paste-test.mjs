// End-to-end copy/paste verification for the ASM web terminal, driving the
// real built client in headless Chrome via raw CDP (no puppeteer installed).
// Companion to attach-button-test.mjs. Three personas, all against a shell
// session with SGR mouse reporting enabled (i.e. behaving like an agent TUI):
//   T1  Linux UA, secure origin (daemon-served bundle on 127.0.0.1):
//       shift+drag select, Ctrl-Shift-C chord (+"Copied" receipt, no SIGINT
//       leak), native paste, plain Ctrl-V stays ^V, right-click copy+clear
//   T2  Mac UA emulation, secure origin: shift+drag AND option+drag select
//       under mouse reporting, native ⌘-C copy event, ⌘-V paste
//   T3  Linux UA, INSECURE origin (LAN IP via vite --host): execCommand
//       fallback with focus retention, native paste without clipboard API
//
//   node scripts/copy-paste-test.mjs <daemonBase> <insecureUrl> <cwd> <chromePort>
//
// Full recipe:
//   (cd client && npm run build)
//   D=/tmp/asm-cp; mkdir -p $D/{data,cfg,rt,cwd,chrome}
//   ASM_BIND=127.0.0.1:4671 ASM_DATA_DIR=$D/data ASM_CONFIG_DIR=$D/cfg \
//     ASM_RUNTIME_DIR=$D/rt ASM_STATIC_DIR=$PWD/client/dist ASM_BACKEND=native \
//     ASM_ASMUX_AUTOSPAWN=0 ./target/debug/asm-daemon &
//   # vite bound to the LAN gives an INSECURE origin while its /api+ws proxy
//   # reaches the daemon from loopback (so no device pairing is needed)
//   (cd client && ASM_DAEMON=http://127.0.0.1:4671 npx vite --port 5199 --host 0.0.0.0) &
//   google-chrome --headless=new --disable-gpu --no-sandbox --disable-dev-shm-usage \
//     --remote-debugging-port=9335 --user-data-dir=$D/chrome about:blank &
//   node scripts/copy-paste-test.mjs 127.0.0.1:4671 http://<LAN-IP>:5199/ $D/cwd 9335
//
// Chrome-harness gotchas encoded below: Network.setCacheDisabled (a cached
// index.html silently runs a stale bundle after rebuilds), and auto-accepting
// Page.javascriptDialogOpening (an unanswered confirm() — e.g. the session
// take-over prompt — blocks every same-process Runtime.evaluate forever).

const daemonBase = process.argv[2] ?? "127.0.0.1:4671";
const insecureUrl = process.argv[3] ?? "http://192.168.0.159:5199/";
const cwd = process.argv[4];
const chromePort = process.argv[5] ?? "9335";
const secureUrl = `http://${daemonBase}/`;

let failures = 0;
const check = (name, cond, extra) => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? "   [" + String(extra).slice(0, 140) + "]" : ""}`);
  if (!cond) failures++;
  return cond;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function browserWs() {
  for (let i = 0; i < 40; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${chromePort}/json/version`);
      const j = await r.json();
      if (j.webSocketDebuggerUrl) return j.webSocketDebuggerUrl;
    } catch {}
    await sleep(250);
  }
  throw new Error("chrome devtools endpoint never came up");
}

function makeConn(wsUrl) {
  const ws = new WebSocket(wsUrl);
  const pending = new Map();
  const logs = []; // every browser/console log line from every tab
  let idc = 1;
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
    }
    if (m.method === "Log.entryAdded") logs.push(m.params.entry.text ?? "");
    if (m.method === "Runtime.consoleAPICalled")
      logs.push((m.params.args ?? []).map((a) => a.value ?? a.description ?? "").join(" "));
    // A confirm()/alert() left open blocks every same-process evaluate —
    // auto-accept (e.g. the session take-over prompt).
    if (m.method === "Page.javascriptDialogOpening") {
      const id = idc++;
      pending.set(id, { resolve: () => {}, reject: () => {} });
      ws.send(JSON.stringify({ id, method: "Page.handleJavaScriptDialog", params: { accept: true }, sessionId: m.sessionId }));
    }
  };
  const ready = new Promise((res) => (ws.onopen = res));
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = idc++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  return { ws, ready, send, logs };
}

const WS_TAP = `(() => {
  window.__sent = [];
  const send = WebSocket.prototype.send;
  WebSocket.prototype.send = function (d) {
    try { if (typeof d === "string") window.__sent.push(d); } catch {}
    return send.apply(this, arguments);
  };
})();`;

const MAC_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

async function main() {
  const conn = makeConn(await browserWs());
  await conn.ready;

  // Clipboard read/write permission for the secure "reader" origin.
  await conn.send("Browser.grantPermissions", {
    origin: secureUrl,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });

  // ---- reader tab: secure origin, used to seed/read the shared clipboard ----
  const reader = await openTab(conn, secureUrl, null);
  const clipRead = async () => {
    await reader.S("Page.bringToFront");
    return await reader.eval(`navigator.clipboard.readText()`);
  };
  const clipSeed = async (text) => {
    await reader.S("Page.bringToFront");
    await reader.eval(`navigator.clipboard.writeText(${JSON.stringify(text)})`);
  };

  // one live shell session, reused by every tab (attach supersedes cleanly)
  const create = await fetch(`http://${daemonBase}/api/sessions`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ agent_plugin_id: "shell", cwd }),
  });
  check("session created", create.ok, create.status);

  // =================== T1: Linux persona, secure origin ===================
  console.log("\n--- T1: Linux persona, secure origin (navigator.clipboard) ---");
  {
    const t = await openTab(conn, secureUrl, null);
    await attachTerminal(t, "MARK1SEC");
    check("T1 secure context (navigator.clipboard present)", await t.eval(`!!navigator.clipboard`));

    // plain drag must go to the app (mouse reporting), not select
    await drag(t, "MARK1SEC", 0);
    check("T1 plain drag reported to app as SGR mouse", await sentHas(t, "[<0;"));
    check("T1 plain drag made no selection", !(await hasSelection(t)));

    // shift+drag selects
    await drag(t, "MARK1SEC", 8);
    check("T1 shift+drag selects under mouse reporting", await hasSelection(t));

    // moving the released mouse used to wipe the selection: ?1003h reports
    // every buttonless move, and reports counted as selection-clearing input
    await moveAcross(t, "MARK1SEC");
    check("T1 motion reports reached the app (?1003h live)", await sentHas(t, "[<35;"));
    check("T1 selection survives any-motion reports", await hasSelection(t));

    // Ctrl-Shift-C copies, flashes receipt, no SIGINT leak
    await clipSeed("RESET1");
    await t.S("Page.bringToFront");
    await key(t, { type: "keyDown", modifiers: 10, key: "C", code: "KeyC", windowsVirtualKeyCode: 67 });
    await key(t, { type: "keyUp", modifiers: 10, key: "C", code: "KeyC", windowsVirtualKeyCode: 67 });
    const flash = await waitFor(t, `document.querySelector('.paste-status--ok')?.textContent || ''`, 2000);
    check("T1 'Copied' receipt flashed", /Copied/.test(flash || ""), flash);
    check("T1 chord did not leak SIGINT (\\x03)", !(await sentHas(t, "\\u0003")));
    const clip1 = await clipRead();
    check("T1 Ctrl-Shift-C copied selection", /MARK1SEC/.test(clip1), clip1);

    // paste: native paste command lands in the pty stream
    await clipSeed("PASTE1-ZZ9");
    await t.S("Page.bringToFront");
    await focusTerm(t);
    await key(t, { type: "keyDown", modifiers: 4, key: "v", code: "KeyV", windowsVirtualKeyCode: 86, commands: ["paste"] });
    await key(t, { type: "keyUp", modifiers: 4, key: "v", code: "KeyV", windowsVirtualKeyCode: 86 });
    await sleep(400);
    check("T1 native paste reached the session input", await sentHas(t, "PASTE1-ZZ9"));

    // plain Ctrl+V stays ^V (0x16) to the app
    await key(t, { type: "keyDown", modifiers: 2, key: "v", code: "KeyV", windowsVirtualKeyCode: 86 });
    await key(t, { type: "keyUp", modifiers: 2, key: "v", code: "KeyV", windowsVirtualKeyCode: 86 });
    await sleep(200);
    check("T1 plain Ctrl+V forwards ^V to app", await sentHas(t, "\\u0016"));

    // right-click: copies, clears selection (so next right-click = browser menu).
    // The mouse travels to the click point first, like a real hand does.
    await drag(t, "MARK1SEC", 8);
    check("T1 re-select for right-click", await hasSelection(t));
    await clipSeed("RESET2");
    await t.S("Page.bringToFront");
    await moveAcross(t, "MARK1SEC");
    await rightClick(t, "MARK1SEC");
    await sleep(400);
    const clip2 = await clipRead();
    check("T1 right-click copied selection", /MARK1SEC/.test(clip2), clip2);
    check("T1 right-click cleared selection", !(await hasSelection(t)));
    await t.close();
  }

  // =================== T2: Mac persona, secure origin ===================
  console.log("\n--- T2: Mac persona (UA emulation) ---");
  {
    const t = await openTab(conn, secureUrl, { userAgent: MAC_UA, platform: "MacIntel" });
    check("T2 page sees MacIntel platform", (await t.eval(`navigator.platform`)) === "MacIntel");
    await attachTerminal(t, "MARK2MAC");

    await drag(t, "MARK2MAC", 0);
    check("T2 plain drag reported to app (mouse reporting active)", await sentHas(t, "[<0;"));
    check("T2 plain drag made no selection", !(await hasSelection(t)));

    // THE FIX: shift+drag must select on macOS too
    await drag(t, "MARK2MAC", 8);
    check("T2 shift+drag selects on macOS (the fix)", await hasSelection(t));

    // ?1003h motion reports and ?1004h focus reports must not deselect
    await moveAcross(t, "MARK2MAC");
    check("T2 selection survives ?1003h motion reports", await hasSelection(t));
    await t.eval(
      `(() => { const ta = document.querySelector('.xterm-helper-textarea'); ta.blur(); ta.focus(); })()`,
    );
    await sleep(200);
    check("T2 focus out was reported (?1004h live)", await sentHas(t, "\\u001b[O"));
    check("T2 selection survives focus in/out reports", await hasSelection(t));

    // ⌘-C equivalent: native copy command served by xterm's own copy listener
    await clipSeed("RESET3");
    await t.S("Page.bringToFront");
    await key(t, { type: "keyDown", modifiers: 4, key: "c", code: "KeyC", windowsVirtualKeyCode: 67, commands: ["copy"] });
    await key(t, { type: "keyUp", modifiers: 4, key: "c", code: "KeyC", windowsVirtualKeyCode: 67 });
    await sleep(400);
    const clip3 = await clipRead();
    check("T2 ⌘-C native copy picked up selection", /MARK2MAC/.test(clip3), clip3);

    // Option+drag also selects (macOptionClickForcesSelection): select a
    // DIFFERENT row (the lowercase target line), re-copy, and require the
    // clipboard to change — proves a fresh selection was made.
    await t.S("Page.bringToFront");
    await drag(t, "mark2mac", 1); // alt held
    check("T2 option+drag selects on macOS", await hasSelection(t));
    await key(t, { type: "keyDown", modifiers: 4, key: "c", code: "KeyC", windowsVirtualKeyCode: 67, commands: ["copy"] });
    await key(t, { type: "keyUp", modifiers: 4, key: "c", code: "KeyC", windowsVirtualKeyCode: 67 });
    await sleep(400);
    const clip3b = await clipRead();
    check("T2 option+drag selection is fresh (copied new row)", /mark2mac/.test(clip3b), clip3b);

    // Claude Code re-asserts its mouse modes on every redraw (spinner ticks
    // included); each DECSET fired onProtocolChange → disable() → clear, so a
    // selection died within a second of any TUI activity. Flush the mouse-
    // report junk the earlier tests fed the prompt line before typing.
    await t.S("Page.bringToFront");
    await focusTerm(t);
    for (let i = 0; i < 2; i++) {
      await key(t, { type: "keyDown", modifiers: 2, key: "u", code: "KeyU", windowsVirtualKeyCode: 85 });
      await key(t, { type: "keyUp", modifiers: 2, key: "u", code: "KeyU", windowsVirtualKeyCode: 85 });
    }
    await key(t, { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
    await key(t, { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await sleep(300);
    await typeText(t, `(sleep 2; printf '\\033[?1000h\\033[?1002h\\033[?1003h') &`);
    await key(t, { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
    await key(t, { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await sleep(300);
    await drag(t, "MARK2MAC", 8);
    check("T2 re-selected before mode re-assert", await hasSelection(t));
    await sleep(2700);
    check("T2 selection survives TUI mode re-assertion", await hasSelection(t));

    // The reported bug, end to end: selection made, mouse travels, right-click
    // over a DIFFERENT word. The clipboard must hold the selection — not the
    // word under the pointer (rightClickSelectsWord defaults ON for macOS and
    // used to replace the selection on the contextmenu's own mousedown).
    await clipSeed("RESET-RC");
    await t.S("Page.bringToFront");
    await moveAcross(t, "mark2mac");
    await rightClick(t, "mark2mac");
    const flashRC = await waitFor(t, `document.querySelector('.paste-status--ok')?.textContent || ''`, 2000);
    check("T2 right-click flashed 'Copied'", /Copied/.test(flashRC || ""), flashRC);
    const clipRC = await clipRead();
    check(
      "T2 right-click copied the selection, not the word under the pointer",
      /MARK2MAC/.test(clipRC) && !/mark2mac/.test(clipRC),
      clipRC,
    );

    // ⌘-V paste
    await clipSeed("PASTE2-QQ7");
    await t.S("Page.bringToFront");
    await focusTerm(t);
    await key(t, { type: "keyDown", modifiers: 4, key: "v", code: "KeyV", windowsVirtualKeyCode: 86, commands: ["paste"] });
    await key(t, { type: "keyUp", modifiers: 4, key: "v", code: "KeyV", windowsVirtualKeyCode: 86 });
    await sleep(400);
    check("T2 ⌘-V paste reached the session input", await sentHas(t, "PASTE2-QQ7"));
    await t.close();
  }

  // ============ T3: Linux persona, INSECURE origin (LAN IP via vite) ============
  console.log("\n--- T3: insecure origin (execCommand fallback) ---");
  {
    const t = await openTab(conn, insecureUrl, null);
    await attachTerminal(t, "MARK3LAN");
    check("T3 insecure context (no navigator.clipboard)", await t.eval(`!navigator.clipboard`));

    await drag(t, "MARK3LAN", 8);
    check("T3 shift+drag selects", await hasSelection(t));

    await clipSeed("RESET4");
    await t.S("Page.bringToFront");
    await key(t, { type: "keyDown", modifiers: 10, key: "C", code: "KeyC", windowsVirtualKeyCode: 67 });
    await key(t, { type: "keyUp", modifiers: 10, key: "C", code: "KeyC", windowsVirtualKeyCode: 67 });
    const flash3 = await waitFor(t, `document.querySelector('.paste-status--ok')?.textContent || ''`, 2000);
    check("T3 'Copied' receipt flashed", /Copied/.test(flash3 || ""), flash3);
    check(
      "T3 focus stayed on the terminal after fallback copy",
      await t.eval(`document.activeElement?.classList?.contains('xterm-helper-textarea') ?? false`),
    );
    const clip4 = await clipRead();
    check("T3 execCommand fallback copied selection", /MARK3LAN/.test(clip4), clip4);

    // paste still native even when clipboard API is absent
    await clipSeed("PASTE3-KK5");
    await t.S("Page.bringToFront");
    await focusTerm(t);
    await key(t, { type: "keyDown", modifiers: 4, key: "v", code: "KeyV", windowsVirtualKeyCode: 86, commands: ["paste"] });
    await key(t, { type: "keyUp", modifiers: 4, key: "v", code: "KeyV", windowsVirtualKeyCode: 86 });
    await sleep(400);
    check("T3 native paste works on insecure origin", await sentHas(t, "PASTE3-KK5"));
    await t.close();
  }

  // Across every tab (T3's dev-mode StrictMode double-mount is the
  // deterministic repro): tearing down a still-connecting socket must not
  // log the browser's close-mid-handshake warning.
  const wsWarn = conn.logs.filter((l) => l.includes("closed before the connection"));
  check("no 'closed before the connection is established' WS warnings", wsWarn.length === 0, wsWarn[0]);

  await reader.close();
  conn.ws.close();
  console.log(failures ? `\n${failures} FAILURE(S)` : "\nALL PASS");
  process.exit(failures ? 1 : 0);
}

// ---------- helpers ----------

async function openTab(conn, url, uaOverride) {
  const { targetId } = await conn.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await conn.send("Target.attachToTarget", { targetId, flatten: true });
  const S = (method, params) => conn.send(method, params, sessionId);
  await S("Runtime.enable");
  await S("Page.enable");
  await S("Log.enable"); // surfaces browser-generated warnings (e.g. WS close-mid-handshake)
  await S("Network.enable");
  await S("Network.setCacheDisabled", { cacheDisabled: true }); // always run the freshly built bundle
  if (uaOverride) await S("Emulation.setUserAgentOverride", uaOverride);
  await S("Page.addScriptToEvaluateOnNewDocument", { source: WS_TAP });
  await S("Page.navigate", { url });
  const tab = {
    S,
    eval: async (expr) => {
      const { result, exceptionDetails } = await S("Runtime.evaluate", {
        expression: expr,
        returnByValue: true,
        awaitPromise: true,
      });
      if (exceptionDetails) throw new Error(exceptionDetails.text + " " + JSON.stringify(exceptionDetails.exception ?? {}));
      return result.value;
    },
    close: () => conn.send("Target.closeTarget", { targetId }).catch(() => {}),
  };
  await waitFor(tab, `document.readyState === 'complete'`, 10000);
  return tab;
}

async function waitFor(tab, expr, ms = 12000) {
  const t0 = Date.now();
  let last;
  while (Date.now() - t0 < ms) {
    last = await tab.eval(expr).catch(() => undefined);
    if (last) return last;
    await sleep(150);
  }
  return last;
}

async function key(tab, params) {
  await tab.S("Input.dispatchKeyEvent", params);
}

async function typeText(tab, s) {
  for (const ch of s) {
    await key(tab, { type: "keyDown", text: ch, key: ch, unmodifiedText: ch });
    await key(tab, { type: "keyUp", key: ch });
  }
}

async function focusTerm(tab) {
  await tab.eval(`document.querySelector('.xterm-helper-textarea')?.focus()`);
}

// Click the session row, wait for the shell, then type a unique marker line and
// (re-)enable SGR mouse reporting so the terminal behaves like an agent TUI.
async function attachTerminal(tab, marker) {
  const row = await waitFor(tab, `!!document.querySelector('.session-row')`);
  check(`row rendered (${marker})`, !!row);
  await tab.eval(`document.querySelector('.session-row').click()`);
  await waitFor(tab, `!!document.querySelector('.xterm-helper-textarea')`);
  await sleep(800); // let the attach snapshot land
  await tab.S("Page.bringToFront");
  await focusTerm(tab);
  // Flush whatever a previous tab left in the pty line buffer (mouse-report
  // garbage, pasted text, even a pending readline quoted-insert from a ^V
  // test): ^U twice — the first is consumed if a quote is pending, the second
  // really kills the line — then Enter for a fresh prompt.
  for (let i = 0; i < 2; i++) {
    await key(tab, { type: "keyDown", modifiers: 2, key: "u", code: "KeyU", windowsVirtualKeyCode: 85 });
    await key(tab, { type: "keyUp", modifiers: 2, key: "u", code: "KeyU", windowsVirtualKeyCode: 85 });
  }
  await key(tab, { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
  await key(tab, { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await sleep(300);
  // The full mode set Claude Code asserts: click+drag+any-motion tracking,
  // focus reporting, SGR encoding. ?1003h is the crucial one — motion reports
  // fire on every buttonless mouse move. The lowercase echo is a second,
  // always-visible target row whose text can't be mistaken for the marker in
  // a clipboard assertion (matching is case-sensitive).
  await typeText(
    tab,
    `echo ${marker}; echo ${marker.toLowerCase()}; printf '\\033[?1000h\\033[?1002h\\033[?1003h\\033[?1004h\\033[?1006h'`,
  );
  await key(tab, { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
  await key(tab, { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  const seen = await waitFor(
    tab,
    `[...document.querySelectorAll('.xterm-rows > div')].some(r => r.textContent.trim() === ${JSON.stringify(marker)})`,
  );
  check(`marker line rendered (${marker})`, !!seen);
}

// Find the row whose text is exactly the marker (or contains it, with
// fuzzy=true) and drag across it. modifiers: 0 plain, 8 shift, 1 alt.
async function drag(tab, marker, modifiers, fuzzy = false) {
  const rect = await tab.eval(`(() => {
    const rows = [...document.querySelectorAll('.xterm-rows > div')];
    const row = rows.find(r => ${fuzzy}
      ? r.textContent.includes(${JSON.stringify(marker)})
      : r.textContent.trim() === ${JSON.stringify(marker)});
    if (!row) return null;
    const r = row.getBoundingClientRect();
    return { x: r.left + 2, y: r.top + r.height / 2, w: Math.min(r.width, 220) };
  })()`);
  if (!rect) throw new Error("marker row not found for drag: " + marker);
  const M = (type, x, extra = {}) =>
    tab.S("Input.dispatchMouseEvent", { type, x, y: rect.y, modifiers, button: "left", buttons: 1, clickCount: 1, ...extra });
  await M("mousePressed", rect.x);
  for (let i = 1; i <= 5; i++) await M("mouseMoved", rect.x + (rect.w * i) / 5);
  await M("mouseReleased", rect.x + rect.w);
  await sleep(150);
}

async function rowRect(tab, marker, fuzzy = false) {
  const rect = await tab.eval(`(() => {
    const rows = [...document.querySelectorAll('.xterm-rows > div')];
    const row = rows.find(r => ${fuzzy}
      ? r.textContent.includes(${JSON.stringify(marker)})
      : r.textContent.trim() === ${JSON.stringify(marker)});
    if (!row) return null;
    const r = row.getBoundingClientRect();
    return { x: r.left + 30, y: r.top + r.height / 2, w: Math.min(r.width, 220) };
  })()`);
  if (!rect) throw new Error("marker row not found: " + marker);
  return rect;
}

// Buttonless mouse travel across the marker's row — under ?1003h every step
// is reported to the app, which historically wiped the selection.
async function moveAcross(tab, marker, fuzzy = false) {
  const rect = await rowRect(tab, marker, fuzzy);
  for (let i = 1; i <= 4; i++) {
    await tab.S("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: rect.x + (rect.w * i) / 4,
      y: rect.y,
      buttons: 0,
    });
  }
  await sleep(150);
}

async function rightClick(tab, marker, fuzzy = false) {
  const rect = await rowRect(tab, marker, fuzzy);
  await tab.S("Input.dispatchMouseEvent", { type: "mousePressed", x: rect.x, y: rect.y, button: "right", buttons: 2, clickCount: 1 });
  await tab.S("Input.dispatchMouseEvent", { type: "mouseReleased", x: rect.x, y: rect.y, button: "right", buttons: 0, clickCount: 1 });
}

async function hasSelection(tab) {
  return await tab.eval(
    `(document.querySelector('.xterm-selection')?.children.length ?? 0) > 0`,
  );
}

async function sentHas(tab, needle) {
  return await tab.eval(`window.__sent.some(f => f.includes(${JSON.stringify(needle)}))`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
