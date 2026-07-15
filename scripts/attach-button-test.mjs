// Headless-Chrome verification of the 📎 attach button, driving the real built
// client bundle served by the daemon. Companion to `paste-test.mjs` (which
// proves the daemon + WS path deterministically); this proves the browser wiring:
// button → file picker → upload → path injected into the PTY.
//
// Covers BOTH attachment kinds, because the picker takes any file now: a PNG
// (which injects the `[pasted image …]` placeholder) and a PDF (which injects
// `[attached file …]`). The PDF round is the one that would have been impossible
// before — the input carried `accept="image/*"` and the change handler dropped
// non-images on the floor without a word.
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
  const pdfPath = join(sb.tmp, "spec.pdf");
  fs.writeFileSync(pdfPath, "%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\n%%EOF\n");

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

  // The picker must offer every file type — an `accept` filter here is exactly
  // what used to make a PDF unselectable in the OS dialog.
  check(
    "file input has no accept filter",
    (await evalJs(
      "document.querySelector('.terminal-host input[type=file]')?.getAttribute('accept') ?? 'NONE'",
    )) === "NONE",
  );

  const pasteDir = `${sb.cwd}/.asm/pastes`;
  const SEL = ".terminal-host input[type=file]";

  // Push a file into the hidden input the way a real picker would, then fire the
  // change event React listens for.
  const pick = async (path) => {
    const { result } = await S("Runtime.evaluate", { expression: `document.querySelector('${SEL}')` });
    if (!result.objectId) return false;
    await S("DOM.setFileInputFiles", { objectId: result.objectId, files: [path] });
    await evalJs(`document.querySelector('${SEL}').dispatchEvent(new Event('change',{bubbles:true}))`);
    await sleep(2500); // let the upload + WS injection complete
    return true;
  };
  const storedWith = (ext) =>
    fs.existsSync(pasteDir) ? fs.readdirSync(pasteDir).filter((f) => f.endsWith(ext)) : [];
  const termText = () => evalJs("document.querySelector('.terminal-host')?.innerText || ''");

  // --- round 1: an image, the original behaviour ---
  check("hidden file input present", await pick(pngPath));
  check("button uploaded a PNG to the daemon", storedWith(".png").length >= 1);
  check(
    "image placeholder echoed into the terminal",
    /pasted image/.test(await termText()),
    (await termText()).replace(/\s+/g, " ").slice(0, 120),
  );

  // --- round 2: a PDF, the newly-allowed kind ---
  await pick(pdfPath);
  const pdfs = storedWith(".pdf");
  check("button uploaded a PDF to the daemon", pdfs.length >= 1, pdfs.join(","));
  check(
    "PDF keeps its original stem in the stored name",
    pdfs.some((f) => f.startsWith("spec-")),
    pdfs.join(","),
  );
  check(
    "file placeholder echoed into the terminal",
    /attached file/.test(await termText()),
    (await termText()).replace(/\s+/g, " ").slice(0, 160),
  );

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the 📎 button uploads and injects both an image and a PDF");
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
