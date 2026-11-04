// Headless-Chrome verification of the mobile "jump to latest" pill (.term-jump),
// at a phone viewport with touch emulation on. The point of the feature is that
// ONE affordance has to serve two scroll models that share the terminal, so this
// drives a real shell session into each shape and asserts both:
//
//   A. TERMINAL-owned scrollback (codex, a shell, any ended-session replay).
//      The drag moves xterm's own viewport. The pill must appear only once the
//      view has left the tail, and scroll it back exactly.
//
//   B. APP-owned scroll (claude, or any TUI holding mouse reporting). The drag
//      never touches the viewport — it leaves as a mouse report and the app
//      redraws its window from its own transcript, so xterm's buffer does not
//      move a single line and cannot see that anything scrolled. The pill must
//      still appear, and tapping it must hand the app back exactly as many
//      wheel-DOWN reports as it was sent wheel-UPs.
//
//   cd client && npm run build         # once, to produce client/dist
//   node scripts/term-jump-test.mjs    # sandboxed: daemon + chrome + sessions
//
// Same-origin loopback ⇒ no token. Node 18+ (global fetch/WS). See also
// docs/mobile-ui.md and docs/terminal-scrollback.md (where the two models are
// diagnosed), plus scripts/touch-select-test.mjs for the gesture layer itself.

import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-jump");

// Where session B's pty writes every byte we send it. A file, not the echoed
// screen: a mouse report is 13-ish glyphs and would wrap across terminal rows,
// so counting it out of the rendered text is a lie waiting to happen.
const REPORTS = "reports.txt";

async function main() {
  await sb.startAppDaemon();
  const mkSession = async () => {
    const { session } = await sb.api("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
    });
    return session;
  };
  const a = await mkSession();
  const b = await mkSession();
  check("two shell sessions created", a.status === "running" && b.status === "running");

  const chrome = await sb.startChrome();
  const page = await chrome.openPage("about:blank");
  const { S, evalJs, waitFor } = page;

  // Phone: 390×844 matches PHONE_MQ's (max-width:599px) and touch emulation makes
  // (pointer: coarse) match — together they mount MobileShell, which owns the pill.
  await S("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 3,
    mobile: true,
  });
  await S("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  await S("Network.setCacheDisabled", { cacheDisabled: true }); // else a stale index.html runs the OLD bundle
  await S("Page.navigate", { url: `${sb.http}/` });
  check("mobile shell mounts at phone width", await waitFor("!!document.querySelector('.mobile-shell')"));

  // --- helpers -------------------------------------------------------------
  // The two rows are indistinguishable in the tree (same agent, same cwd), so
  // they are taken by position. Which is which never matters: each half of the
  // test is self-contained, and they share the one sandbox cwd.
  const openSession = async (i) => {
    await waitFor("document.querySelectorAll('.session-row').length >= 2");
    await evalJs(`document.querySelectorAll('.session-row')[${i}].click()`);
    await waitFor("!!document.querySelector('.mobile-term-header')");
    await waitFor(
      "!document.querySelector('.terminal-loading') && (document.querySelector('.terminal-mount .xterm-rows')?.innerText||'').trim().length > 0",
    );
  };
  const enter = async () => {
    await S("Input.dispatchKeyEvent", {
      type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r",
    });
    await S("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  };
  const run = async (cmd) => {
    await evalJs("document.querySelector('.xterm-helper-textarea').focus()");
    await S("Input.insertText", { text: cmd });
    await enter();
  };
  // Finger down ⇒ content moves down ⇒ the view travels toward OLDER output.
  // That is a wheel-UP, whichever model consumes it.
  const swipeBack = async (steps = 10) => {
    const x = 190;
    const y0 = 300;
    await S("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y: y0 }] });
    for (let i = 1; i <= steps; i++) {
      await S("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x, y: y0 + i * 18 }] });
      await sleep(16);
    }
    await S("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await sleep(200);
  };
  const pill = "document.querySelector('.term-jump')";
  const tapPill = async () => {
    await evalJs(`${pill}.click()`);
    await sleep(400);
  };
  const SCROLL_TOP = "Math.round(document.querySelector('.xterm-viewport').scrollTop)";
  const SCROLL_MAX =
    "Math.round(document.querySelector('.xterm-viewport').scrollHeight - document.querySelector('.xterm-viewport').clientHeight)";

  // ================================================================== A
  // Terminal-owned scrollback: xterm's viewport is the scroll.
  await openSession(0);
  await run("seq 1 300");
  await waitFor("/\\b300\\b/.test(document.querySelector('.terminal-host').innerText)", 8000);
  await sleep(300);

  check("A pill hidden while the view sits at the live tail", await evalJs(`!${pill}`));

  await swipeBack();
  const scrolled = await evalJs(SCROLL_TOP);
  const bottom = await evalJs(SCROLL_MAX);
  check("A a back-drag really scrolled the viewport", scrolled < bottom, `scrollTop=${scrolled} max=${bottom}`);
  check("A pill appears once the view leaves the tail", await waitFor(`!!${pill}`, 2000));

  await tapPill();
  const returned = await evalJs(SCROLL_TOP);
  check("A tapping it returns the viewport to the bottom", returned === bottom, `scrollTop=${returned} max=${bottom}`);
  check("A pill hides itself again at the tail", await waitFor(`!${pill}`, 2000));

  // ================================================================== B
  // App-owned scroll: the claude shape. `?1049h` = alternate screen, `?1000h` +
  // `?1006h` = mouse reporting, SGR-encoded — exactly what claude arms and
  // re-arms (docs/terminal-scrollback.md). `stty -icanon -echo` + `cat` makes the
  // pty write every byte it is sent straight to a file, unbuffered and unechoed:
  // the ground truth for what the "app" actually received.
  await evalJs("window.history.back()");
  await waitFor("!!document.querySelector('.mobile-home-header')");
  await openSession(1);
  await run(`printf '\\e[?1049h\\e[?1000h\\e[?1006h'; stty -icanon -echo; cat > ${REPORTS}`);
  // The alternate screen comes up blank, and with echo off nothing lands on it —
  // so an empty grid where the command line just was IS the buffer switch.
  check(
    "B terminal entered the alternate screen",
    await waitFor("document.querySelector('.terminal-host').innerText.trim().length === 0", 5000),
  );
  check(
    "B app took the mouse (xterm forwards the wheel to it)",
    await waitFor("document.querySelector('.xterm').classList.contains('enable-mouse-events')", 4000),
  );

  const reportsFile = join(sb.cwd, REPORTS);
  const countReports = () => {
    const raw = fs.existsSync(reportsFile) ? fs.readFileSync(reportsFile, "latin1") : "";
    return {
      up: (raw.match(/\x1b\[<64;/g) || []).length,
      down: (raw.match(/\x1b\[<65;/g) || []).length,
    };
  };

  check("B pill hidden before anything is scrolled", await evalJs(`!${pill}`));
  const topBefore = await evalJs(SCROLL_TOP);

  await swipeBack();
  await sleep(300);
  const sent = countReports();
  check("B the back-drag reached the app as wheel-UP reports", sent.up > 0, `${sent.up} × \\e[<64`);
  check(
    "B xterm's own viewport did NOT move (nothing for it to scroll)",
    (await evalJs(SCROLL_TOP)) === topBefore,
    `scrollTop=${topBefore}`,
  );
  check("B pill appears anyway — the app's scroll is tracked", await waitFor(`!!${pill}`, 2000));

  await tapPill();
  await sleep(400);
  const back = countReports();
  check(
    "B tapping it hands the app back exactly as many wheel-DOWNs",
    back.down === sent.up && back.up === sent.up,
    `up=${back.up} down=${back.down}`,
  );
  check("B pill hides itself again", await waitFor(`!${pill}`, 2000));

  // A second drag re-arms it: the counter is state, not a one-shot.
  await swipeBack(4);
  check("B pill returns on the next back-drag", await waitFor(`!!${pill}`, 2000));

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("jump-to-latest works for both scroll models on a phone");
}

main()
  .then((pass) => {
    sb.cleanup();
    process.exit(pass ? 0 : 1);
  })
  .catch((e) => {
    console.error(e);
    sb.cleanup();
    process.exit(1);
  });
