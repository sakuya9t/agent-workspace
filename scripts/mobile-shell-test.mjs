// Headless-Chrome verification of the mobile adaptive shell (MOB phases 1-3),
// driving the real built client bundle served by the daemon at a phone
// viewport. Proves the browser wiring end-to-end: device switch → session tap →
// full-screen terminal + key bar → key-bar/​Ctrl-latch input over the WS →
// details sheet → back navigation.
//
//   node scripts/mobile-shell-test.mjs <base host:port> <chromePort> <pngOut>
//
// Recipe (see also docs/mobile-ui.md and build-run-env memory):
//   (cd client && npm run build)
//   D=/tmp/asm-mob; mkdir -p $D/{data,cfg,rt,cwd,chrome}; git -C $D/cwd init -q
//   ASM_BIND=127.0.0.1:4671 ASM_DATA_DIR=$D/data ASM_CONFIG_DIR=$D/cfg \
//     ASM_RUNTIME_DIR=$D/rt ASM_STATIC_DIR=$PWD/client/dist ASM_BACKEND=native \
//     ASM_ASMUX_AUTOSPAWN=0 ./target/debug/asm-daemon &
//   curl -sX POST 127.0.0.1:4671/api/sessions -H 'content-type: application/json' \
//     -d "{\"agent_plugin_id\":\"shell\",\"cwd\":\"$D/cwd\"}"
//   google-chrome --headless=new --disable-gpu --no-sandbox --disable-dev-shm-usage \
//     --remote-debugging-port=9334 --user-data-dir=$D/chrome about:blank &
//   node scripts/mobile-shell-test.mjs 127.0.0.1:4671 9334 $D/term.png
//
// Same-origin (default `local` daemon has baseUrl="" so loopback trust = no
// token). Node 18+ (global fetch/WS).

import fs from "node:fs";

const [base, chromePort, pngOut] = [
  process.argv[2] ?? "127.0.0.1:4671",
  process.argv[3] ?? "9334",
  process.argv[4] ?? "/tmp/asm-mob/term.png",
];
const appUrl = `http://${base}/`;

let failures = 0;
const check = (name, cond, extra) => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`);
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
    } catch {
      /* not up yet */
    }
    await sleep(250);
  }
  throw new Error("chrome devtools endpoint never came up");
}

function makeConn(wsUrl) {
  const ws = new WebSocket(wsUrl);
  const pending = new Map();
  let idc = 1;
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
    }
  };
  const ready = new Promise((res) => (ws.onopen = res));
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = idc++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  return { ws, ready, send };
}

async function main() {
  const conn = makeConn(await browserWs());
  await conn.ready;

  const { targetId } = await conn.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await conn.send("Target.attachToTarget", { targetId, flatten: true });
  const S = (method, params) => conn.send(method, params, sessionId);
  await S("Runtime.enable");
  await S("DOM.enable");
  await S("Page.enable");
  // Emulate a phone: 390×844 triggers the (max-width:599px) clause in PHONE_MQ,
  // and touch emulation makes (pointer: coarse) match too.
  await S("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 3,
    mobile: true,
  });
  await S("Emulation.setTouchEmulationEnabled", { enabled: true });
  await S("Page.navigate", { url: appUrl });

  const evalJs = async (expr) => {
    const { result, exceptionDetails } = await S("Runtime.evaluate", {
      expression: expr,
      returnByValue: true,
      awaitPromise: true,
    });
    if (exceptionDetails) throw new Error(exceptionDetails.text + " :: " + expr);
    return result.value;
  };
  const waitFor = async (expr, ms = 12000) => {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      if (await evalJs(expr)) return true;
      await sleep(250);
    }
    return false;
  };
  const clickKb = (label) =>
    evalJs(
      `(() => { const b=[...document.querySelectorAll('.term-keybar .kb')].find(x=>x.textContent.trim()===${JSON.stringify(
        label,
      )}); if(b){b.click();return true;} return false; })()`,
    );

  // 1. Device switch → mobile home.
  check("mobile shell mounts at phone width", await waitFor("!!document.querySelector('.mobile-shell')"));
  check("home header renders", await evalJs("!!document.querySelector('.mobile-home-header')"));
  check("desktop 3-pane NOT mounted", await evalJs("!document.querySelector('.workspace')"));
  check("shared session tree present in home", await waitFor("!!document.querySelector('.session-row')"));

  // 2. Tap a session → full-screen terminal + key bar.
  await evalJs("document.querySelector('.session-row').click()");
  check("terminal screen header shows", await waitFor("!!document.querySelector('.mobile-term-header')"));
  check("home no longer mounted (pushed screen)", await evalJs("!document.querySelector('.mobile-home-header')"));
  check("key bar rendered for live session", await waitFor("!!document.querySelector('.term-keybar')"));
  const kbCount = await evalJs("document.querySelectorAll('.term-keybar .kb').length");
  // Esc Tab ⇧Tab Ctrl ^C ↑ ↓ ← → ⌨ Paste Copy
  check("key bar has all keys", kbCount === 12, `count=${kbCount}`);

  // 3. Terminal is live (shell prompt painted).
  check(
    "xterm painted output",
    await waitFor("(document.querySelector('.terminal-host')?.innerText||'').trim().length > 0"),
  );

  // 4. ⌨ focuses the xterm textarea (summons keyboard on a real device).
  await evalJs("document.querySelector('.term-keybar .kbd').click()");
  check(
    "keyboard key focuses xterm textarea",
    await waitFor("document.activeElement?.classList.contains('xterm-helper-textarea')", 3000),
  );

  // 5. Key-bar write path: ^C sends \\x03 → the tty echoes "^C".
  const beforeC = await evalJs("document.querySelector('.terminal-host').innerText");
  await clickKb("^C");
  check(
    "^C key sent SIGINT (tty echoed ^C)",
    await waitFor(
      `document.querySelector('.terminal-host').innerText.length > ${beforeC.length} && /\\^C/.test(document.querySelector('.terminal-host').innerText)`,
      4000,
    ),
  );

  // 6. Ctrl latch: arm → next typed key becomes its control code, then resets.
  await evalJs(
    "[...document.querySelectorAll('.term-keybar .kb')].find(x=>x.textContent.trim()==='Ctrl').click()",
  );
  check(
    "Ctrl tap arms the latch (visual)",
    await evalJs(
      "[...document.querySelectorAll('.term-keybar .kb')].find(x=>x.textContent.trim()==='Ctrl').classList.contains('on')",
    ),
  );
  // Type a plain 'c' through the xterm textarea; armed latch → \\x03 → ^C again.
  const beforeLatch = await evalJs("document.querySelector('.terminal-host').innerText");
  await evalJs("document.querySelector('.xterm-helper-textarea').focus()");
  await S("Input.insertText", { text: "c" });
  check(
    "armed Ctrl transforms next key to control code",
    await waitFor(
      `document.querySelector('.terminal-host').innerText.length > ${beforeLatch.length}`,
      4000,
    ),
  );
  check(
    "one-shot latch auto-reset after the key",
    await evalJs(
      "!([...document.querySelectorAll('.term-keybar .kb')].find(x=>x.textContent.trim()==='Ctrl').classList.contains('on'))",
    ),
  );

  // Screenshot the terminal + key bar for the record.
  const shot = await S("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(pngOut, Buffer.from(shot.data, "base64"));
  console.log(`screenshot: ${pngOut}`);

  // 7. Details sheet opens over the terminal, back closes it.
  await evalJs("document.querySelector('.mobile-details-btn').click()");
  check("details sheet opens on ⓘ", await waitFor("!!document.querySelector('.details-sheet')"));
  check("RightPanel mounted in the sheet", await evalJs("!!document.querySelector('.details-sheet .panel.right')"));
  check("terminal still mounted under the sheet", await evalJs("!!document.querySelector('.term-keybar')"));
  await evalJs("window.history.back()");
  check("back closes the sheet, stays on terminal", await waitFor("!document.querySelector('.details-sheet') && !!document.querySelector('.mobile-term-header')"));

  // 8. Back returns to the home screen (clears the active session).
  await evalJs("window.history.back()");
  check("back returns to sessions home", await waitFor("!!document.querySelector('.mobile-home-header') && !document.querySelector('.mobile-term-header')"));

  await conn.send("Target.closeTarget", { targetId }).catch(() => {});
  conn.ws.close();
  console.log(failures ? `\n${failures} FAILURE(S)` : "\nALL PASS");
  process.exit(failures ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
