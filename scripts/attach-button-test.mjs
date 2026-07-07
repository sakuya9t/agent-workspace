// Headless-Chrome verification of the 📎 attach-image button, driving the real
// built client bundle served by the daemon. Companion to `paste-test.mjs`
// (which proves the daemon + WS path deterministically); this proves the
// browser wiring: button → file picker → upload → path injected into the PTY.
//
//   node scripts/attach-button-test.mjs <base host:port> <cwd> <pngPath> <chromePort>
//
// Full recipe (see also docs/image-paste.md → "Verifying"):
//   # 1. build the client bundle
//   (cd client && npm run build)
//   # 2. start a THROWAWAY daemon serving the bundle — NOT on 4600, which is
//   #    usually the real running daemon; pick a free port and set ASM_STATIC_DIR
//   D=/tmp/asm-btn; mkdir -p $D/{data,cfg,rt,cwd,chrome}
//   printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > $D/shot.png
//   ASM_BIND=127.0.0.1:4671 ASM_DATA_DIR=$D/data ASM_CONFIG_DIR=$D/cfg \
//     ASM_RUNTIME_DIR=$D/rt ASM_STATIC_DIR=$PWD/client/dist ASM_BACKEND=native \
//     ASM_ASMUX_AUTOSPAWN=0 ./target/debug/asm-daemon &
//   # 3. create a live session in $D/cwd (POST /api/sessions {agent_plugin_id:"shell", cwd})
//   # 4. launch headless chrome with a debug port
//   google-chrome --headless=new --disable-gpu --no-sandbox --disable-dev-shm-usage \
//     --remote-debugging-port=9334 --user-data-dir=$D/chrome about:blank &
//   # 5. node scripts/attach-button-test.mjs 127.0.0.1:4671 $D/cwd $D/shot.png 9334
//
// Requires: the app served same-origin (the default `local` daemon has
// baseUrl="" so loopback trust means no token). The session tree is expanded by
// default, so `.session-row` is directly clickable. Node 18+ (global fetch/WS).

import fs from "node:fs";

const [base, cwd, pngPath, chromePort] = [
  process.argv[2] ?? "127.0.0.1:4671",
  process.argv[3],
  process.argv[4],
  process.argv[5] ?? "9334",
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

  // Open the app in a fresh tab and attach a flat CDP session to it.
  const { targetId } = await conn.send("Target.createTarget", { url: appUrl });
  const { sessionId } = await conn.send("Target.attachToTarget", { targetId, flatten: true });
  const S = (method, params) => conn.send(method, params, sessionId);
  await S("Runtime.enable");
  await S("DOM.enable");
  await S("Page.enable");

  const evalJs = async (expr) => {
    const { result } = await S("Runtime.evaluate", {
      expression: expr,
      returnByValue: true,
      awaitPromise: true,
    });
    return result.value;
  };
  const waitFor = async (expr, ms = 12000) => {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      if (await evalJs(expr)) return true;
      await sleep(300);
    }
    return false;
  };

  // The tree is expanded by default, so the session row is directly present.
  check("session row rendered", await waitFor("!!document.querySelector('.session-row')"));
  await evalJs("document.querySelector('.session-row').click()");

  check("📎 attach button rendered when live", await waitFor("!!document.querySelector('.term-attach')"));
  check(
    "attach button has an accessible label",
    await evalJs("document.querySelector('.term-attach')?.getAttribute('aria-label') || ''"),
  );

  // Push a file into the hidden input the way a real picker would, then fire the
  // change event React listens for.
  const { result } = await S("Runtime.evaluate", {
    expression: "document.querySelector('.terminal-host input[type=file]')",
  });
  check("hidden file input present", !!result.objectId);
  if (result.objectId) {
    await S("DOM.setFileInputFiles", { objectId: result.objectId, files: [pngPath] });
    await evalJs(
      "document.querySelector('.terminal-host input[type=file]').dispatchEvent(new Event('change',{bubbles:true}))",
    );
  }

  await sleep(2500); // let the upload + WS injection complete

  const pasteDir = `${cwd}/.asm/pastes`;
  const stored = fs.existsSync(pasteDir)
    ? fs.readdirSync(pasteDir).filter((f) => f.endsWith(".png"))
    : [];
  check("button uploaded a PNG to the daemon", stored.length >= 1, stored.join(","));

  const termText = await evalJs("document.querySelector('.terminal-host')?.innerText || ''");
  check(
    "placeholder echoed into the terminal",
    /pasted image/.test(termText),
    termText.replace(/\s+/g, " ").slice(0, 120),
  );

  await conn.send("Target.closeTarget", { targetId }).catch(() => {});
  conn.ws.close();
  console.log(failures ? `\n${failures} FAILURE(S)` : "\nALL PASS");
  process.exit(failures ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
