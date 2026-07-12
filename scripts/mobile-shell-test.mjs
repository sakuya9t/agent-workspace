// Headless-Chrome verification of the mobile adaptive shell (MOB phases 1-3),
// driving the real built client bundle served by the daemon at a phone
// viewport. Proves the browser wiring end-to-end: device switch → session tap →
// full-screen terminal + key bar → key-bar/​Ctrl-latch input over the WS →
// details sheet → back navigation.
//
//   cd client && npm run build          # once, to produce client/dist
//   node scripts/mobile-shell-test.mjs  # sandboxed: daemon + chrome + session
//
// It used to require a hand-rolled recipe (throwaway daemon on :4671 with
// ASM_STATIC_DIR, a curl to make a session, chrome with a debug port, three
// argv). That is now `createSandbox()` — see scripts/lib/testenv.mjs. See also
// docs/mobile-ui.md.
//
// Same-origin (default `local` daemon has baseUrl="" so loopback trust = no
// token). Node 18+ (global fetch/WS).

import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-mob");
const pngOut = process.argv[2] ?? null; // optional: where to drop the screenshot

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

  // Screenshot the terminal + key bar for the record. Lands in the sandbox unless
  // an explicit path was given (the sandbox is deleted on cleanup).
  const shot = await S("Page.captureScreenshot", { format: "png" });
  const shotPath = pngOut ?? join(sb.tmp, "term.png");
  fs.writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
  console.log(`screenshot: ${shotPath}`);

  // 7. Details sheet opens over the terminal, back closes it.
  await evalJs("document.querySelector('.mobile-details-btn').click()");
  check("details sheet opens on ⓘ", await waitFor("!!document.querySelector('.details-sheet')"));
  check("RightPanel mounted in the sheet", await evalJs("!!document.querySelector('.details-sheet .panel.right')"));
  check("no 'Continue in VS Code' on mobile", await evalJs("!document.querySelector('.details-sheet .vscode-btn')"));
  check("terminal still mounted under the sheet", await evalJs("!!document.querySelector('.term-keybar')"));
  await evalJs("window.history.back()");
  check("back closes the sheet, stays on terminal", await waitFor("!document.querySelector('.details-sheet') && !!document.querySelector('.mobile-term-header')"));

  // 8. Back returns to the home screen (clears the active session).
  await evalJs("window.history.back()");
  check("back returns to sessions home", await waitFor("!!document.querySelector('.mobile-home-header') && !document.querySelector('.mobile-term-header')"));

  // 9. Desktop mode (wide viewport): the 3-pane shell renders and the VS Code
  //    affordance IS present — the parity break is mobile-only.
  await S("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
  await S("Emulation.setTouchEmulationEnabled", { enabled: false });
  await S("Page.navigate", { url: appUrl });
  check("desktop 3-pane mounts at wide width", await waitFor("!!document.querySelector('.workspace')"));
  await waitFor("!!document.querySelector('.session-row')");
  await evalJs("document.querySelector('.session-row').click()");
  check(
    "desktop right panel DOES show Continue in VS Code",
    await waitFor("!!document.querySelector('.panel.right .vscode-btn')", 6000),
  );

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the mobile adaptive shell works end-to-end in a real browser");
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
