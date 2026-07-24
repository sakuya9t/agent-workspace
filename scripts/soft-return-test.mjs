// Headless-Chrome verification of the soft return: Shift+Enter puts a NEWLINE
// in the agent's composer instead of sending the message, and plain Enter still
// sends. Drives the real built client bundle served by the daemon.
//
//   cd client && npm run build          # once, to produce client/dist
//   node scripts/soft-return-test.mjs   # sandboxed: daemon + chrome + session
//
// Two independent layers are checked, because either alone can lie:
//
//   1. THE FRAME. A tap on WebSocket.prototype.send records what the client
//      actually transmits. Shift+Enter must put `\x1b\r` on the wire and must
//      NOT also emit the bare `\r` xterm would otherwise send — a handler that
//      adds the soft return without swallowing the CR would still submit.
//   2. THE BYTES AT THE PTY. The session runs `cat -v`, which prints control
//      characters visibly, so an ESC that survives daemon → pty shows up on
//      screen as `^[`. This is the half that proves delivery, not just intent.
//
// What the agent TUIs then DO with `\x1b\r` was verified out-of-band against
// claude 2.1.219, codex 0.144.6 and opencode 1.17.18 (all three insert a
// newline) — see SOFT_RETURN in client/src/terminalTypes.ts. A `shell` session
// is used here precisely because it has no composer: it is the one target where
// the bytes themselves are observable.
//
// Same-origin on loopback (baseUrl="" ⇒ loopback trust ⇒ no token). Node 18+.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-soft");

// Exactly what TerminalView puts on the wire: `{"t":"i","d":"\r"}`.
const SOFT_RETURN_FRAME = JSON.stringify({ t: "i", d: "\x1b\r" });
const ENTER_FRAME = JSON.stringify({ t: "i", d: "\r" });
const MARKER = "SOFTRETURNOK";

// Installed before any app code runs, so no send can slip past it.
const WS_TAP = `
  window.__sent = [];
  const nativeSend = WebSocket.prototype.send;
  WebSocket.prototype.send = function (data) {
    if (typeof data === "string") window.__sent.push(data);
    return nativeSend.call(this, data);
  };
`;

async function main() {
  await sb.startAppDaemon();
  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  check("session created for the UI", session.status === "running", session.id.slice(0, 8));

  const appUrl = `${sb.http}/`;
  const chrome = await sb.startChrome();
  const page = await chrome.openPage("about:blank");
  const { S, evalJs, waitFor } = page;
  await S("Page.addScriptToEvaluateOnNewDocument", { source: WS_TAP });

  // A real Enter keystroke, as Chrome delivers one. `text` matters: it is what
  // the browser would insert into xterm's textarea if the keydown were left
  // undefaulted, so sending it is what makes "the CR was swallowed" a real
  // claim rather than an artefact of a stripped-down synthetic event.
  // (modifiers: 2 Ctrl, 4 Meta, 8 Shift)
  const enterKey = async (modifiers) => {
    const base = { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13, modifiers };
    await S("Input.dispatchKeyEvent", { ...base, type: "keyDown", text: "\r" });
    await S("Input.dispatchKeyEvent", { ...base, type: "keyUp" });
    await sleep(400);
  };
  const focusTerm = () => evalJs("document.querySelector('.xterm-helper-textarea').focus()");
  const sent = () => evalJs("window.__sent.slice()");
  const clearSent = () => evalJs("window.__sent.length = 0");
  const termText = () => evalJs("document.querySelector('.terminal-host')?.innerText || ''");
  /** How many visible `^[` (ESC, rendered by `cat -v` and by the tty's echoctl). */
  const escCount = async () => (await termText()).split("^[").length - 1;

  // --- Desktop: the Shift+Enter chord ---
  await S("Page.navigate", { url: appUrl });
  check("session row rendered", await waitFor("!!document.querySelector('.session-row')"));
  await evalJs("document.querySelector('.session-row').click()");
  check(
    "terminal live (shell prompt painted)",
    await waitFor(
      "!document.querySelector('.terminal-loading') && (document.querySelector('.terminal-mount .xterm-rows')?.innerText||'').trim().length > 0",
    ),
  );

  // 1. Plain Enter still submits — the whole point is that this did not change.
  await focusTerm();
  await clearSent();
  await S("Input.insertText", { text: `echo ${MARKER}` });
  await enterKey(0);
  check(
    "plain Enter sends a bare CR",
    (await sent()).includes(ENTER_FRAME),
    ENTER_FRAME,
  );
  check(
    "plain Enter ran the command (shell printed the marker on its own line)",
    await waitFor(
      `new RegExp("^${MARKER}$","m").test(document.querySelector('.terminal-host').innerText)`,
      6000,
    ),
  );

  // 2. `cat -v` makes control bytes visible, so the ESC is observable at the pty.
  await focusTerm();
  await S("Input.insertText", { text: "cat -v" });
  await enterKey(0);
  await sleep(600);
  const escBefore = await escCount();

  // 3. Shift+Enter: soft return on the wire, and no CR alongside it.
  await clearSent();
  await focusTerm();
  await enterKey(8);
  const afterShift = await sent();
  check(
    "Shift+Enter sends ESC+CR",
    afterShift.includes(SOFT_RETURN_FRAME),
    JSON.stringify(afterShift),
  );
  check(
    "Shift+Enter does NOT also send a bare CR (xterm's CR is swallowed)",
    !afterShift.includes(ENTER_FRAME),
  );
  const escAfter = await escCount();
  check(
    "the ESC reached the pty (visible as ^[)",
    escAfter > escBefore,
    `${escBefore} -> ${escAfter}`,
  );

  // 4. Modified variants stay out of it: Ctrl+Enter and ⌘/Alt+Enter are not the
  //    soft return, and must fall through to xterm untouched.
  await clearSent();
  await focusTerm();
  await enterKey(2); // Ctrl
  check(
    "Ctrl+Enter is not claimed as a soft return",
    !(await sent()).includes(SOFT_RETURN_FRAME),
  );

  // --- Phone: the ⇧⏎ key-bar button ---
  // A soft keyboard cannot type the chord at all, so on a phone the button IS
  // the feature. Same session (cat -v still running), so the ^[ check holds.
  await S("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 3,
    mobile: true,
  });
  await S("Emulation.setTouchEmulationEnabled", { enabled: true });
  await S("Page.navigate", { url: appUrl });
  check("mobile shell mounts at phone width", await waitFor("!!document.querySelector('.mobile-shell')"));
  check("session row rendered on phone home", await waitFor("!!document.querySelector('.session-row')"));
  await evalJs("document.querySelector('.session-row').click()");
  check("key bar rendered for live session", await waitFor("!!document.querySelector('.term-keybar')"));
  check(
    "terminal reattached (snapshot painted)",
    await waitFor(
      "!document.querySelector('.terminal-loading') && (document.querySelector('.terminal-mount .xterm-rows')?.innerText||'').trim().length > 0",
    ),
  );

  const softBefore = await escCount();
  await clearSent();
  const tapped = await evalJs(
    `(() => { const b=[...document.querySelectorAll('.term-keybar .kb')].find(x=>x.textContent.trim()==='⇧⏎');
       if(b){b.click();return true;} return false; })()`,
  );
  check("⇧⏎ button present in the key bar", tapped);
  await sleep(600);
  check(
    "⇧⏎ button sends ESC+CR",
    (await sent()).includes(SOFT_RETURN_FRAME),
  );
  const softAfter = await escCount();
  check(
    "the button's ESC reached the pty (visible as ^[)",
    softAfter > softBefore,
    `${softBefore} -> ${softAfter}`,
  );

  return report("Shift+Enter (and the ⇧⏎ key) insert a soft return; Enter still sends");
}

let ok = false;
try {
  ok = await main();
} catch (e) {
  console.error("ERROR", e);
} finally {
  sb.cleanup();
}
process.exit(ok ? 0 : 1);
