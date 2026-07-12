// Headless-Chrome verification of the 📎 attach-image button, driving the real
// built client bundle served by the daemon. Companion to `paste-test.mjs` (which
// proves the daemon + WS path deterministically); this proves the browser wiring:
// button → file picker → upload → path injected into the PTY.
//
//   cd client && npm run build          # once, to produce client/dist
//   node scripts/attach-button-test.mjs # sandboxed: daemon + chrome + session
//
// It used to require a five-step manual ritual (build the bundle, hand-start a
// throwaway daemon on :4671 with ASM_STATIC_DIR, create a session by curl, launch
// chrome with a debug port, then pass four argv). All of that is now
// `createSandbox()` — see scripts/lib/testenv.mjs. Skipped rituals are how a test
// ends up pointed at the real daemon, which is what cost six sessions on
// 2026-07-12.
//
// The app is served same-origin on loopback (baseUrl="" ⇒ loopback trust ⇒ no
// token). The session tree is expanded by default, so `.session-row` is directly
// clickable. Node 18+ (global fetch/WS).

import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, sleep, TINY_PNG } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-btn");

async function main() {
  await sb.startAppDaemon();

  // A live session for the UI to attach to, in the sandbox cwd.
  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  check("session created for the UI", session.status === "running", session.id.slice(0, 8));

  const pngPath = join(sb.tmp, "shot.png");
  fs.writeFileSync(pngPath, TINY_PNG);

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { S, evalJs, waitFor } = page;

  // The tree is expanded by default, so the session row is directly present.
  check("session row rendered", await waitFor("!!document.querySelector('.session-row')"));
  await evalJs("document.querySelector('.session-row').click()");

  check(
    "📎 attach button rendered when live",
    await waitFor("!!document.querySelector('.term-attach')"),
  );
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

  const pasteDir = `${sb.cwd}/.asm/pastes`;
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

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the 📎 button uploads and injects a real image");
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
