// Headless-Chrome verification of the Details panel's "Upload files" button,
// driving the real built client bundle served by the daemon. Companion to
// `workspace-upload-test.mjs` (which proves the daemon endpoint deterministically);
// this proves the browser wiring: button → picker → upload → uploads/<name> on
// disk → the panel reports where it went.
//
//   cd client && npm run build                  # once, to produce client/dist
//   node scripts/workspace-upload-ui-test.mjs   # sandboxed: daemon + chrome + session
//
// The load-bearing round is the **replace prompt**. A workspace upload keeps the
// user's filename, so a second upload of the same name collides; the daemon
// answers 409 and the client turns that into a `confirm()`. A CDP client that
// doesn't answer dialogs deadlocks the renderer on that confirm — every
// subsequent `Runtime.evaluate` hangs — so this test installs a dialog handler
// and asserts on what was actually asked, which is also how it proves the prompt
// fired at all rather than the client silently clobbering the file.
//
// The app is served same-origin on loopback (baseUrl="" ⇒ loopback trust ⇒ no
// token). The session tree is expanded by default, so `.session-row` is directly
// clickable. Node 18+ (global fetch/WS).

import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-wsup-ui");

const PDF = "%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\n%%EOF\n";

async function main() {
  await sb.startAppDaemon();

  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  check("session created for the UI", session.status === "running", session.id.slice(0, 8));

  const specPath = join(sb.tmp, "spec.pdf");
  fs.writeFileSync(specPath, PDF);
  const notesPath = join(sb.tmp, "notes.txt");
  fs.writeFileSync(notesPath, "some notes");

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { S, evalJs, waitFor } = page;

  // cdpConnect only routes *replies* (messages with an id); events are dropped.
  // Compose onto its handler rather than replacing it, so command replies keep
  // resolving while we also see dialogs.
  const dialogs = [];
  const prior = chrome.ws.onmessage;
  chrome.ws.onmessage = (ev) => {
    prior(ev);
    const m = JSON.parse(ev.data);
    if (m.method === "Page.javascriptDialogOpening") {
      dialogs.push(m.params.message);
      // Accept: this is the "yes, replace it" branch under test.
      void chrome.send("Page.handleJavaScriptDialog", { accept: true }, m.sessionId);
    }
  };

  check("session row rendered", await waitFor("!!document.querySelector('.session-row')"));
  await evalJs("document.querySelector('.session-row').click()");

  check(
    "upload button rendered in Details",
    await waitFor("!!document.querySelector('.upload-row button')"),
  );
  check(
    "button says what it does",
    /upload/i.test(await evalJs("document.querySelector('.upload-row button')?.innerText || ''")),
    await evalJs("document.querySelector('.upload-row button')?.innerText || ''"),
  );

  const SEL = ".panel.right input[type=file]";
  check(
    "the panel's file input takes any type and several at once",
    (await evalJs(`document.querySelector('${SEL}')?.getAttribute('accept') ?? 'NONE'`)) ===
      "NONE" && (await evalJs(`!!document.querySelector('${SEL}')?.multiple`)),
  );

  // Push files into the hidden input the way a real picker would, then fire the
  // change event React listens for.
  const pick = async (paths) => {
    const { result } = await S("Runtime.evaluate", { expression: `document.querySelector('${SEL}')` });
    if (!result.objectId) return false;
    await S("DOM.setFileInputFiles", { objectId: result.objectId, files: paths });
    await evalJs(`document.querySelector('${SEL}').dispatchEvent(new Event('change',{bubbles:true}))`);
    await sleep(2500); // let the upload(s) complete
    return true;
  };
  const panelText = () => evalJs("document.querySelector('.details')?.innerText || ''");
  const uploaded = (name) => join(sb.cwd, "uploads", name);

  // --- round 1: two files at once, the multi-select case ---
  check("hidden file input present", await pick([specPath, notesPath]));
  check(
    "both files landed in uploads/ under their own names",
    fs.existsSync(uploaded("spec.pdf")) && fs.existsSync(uploaded("notes.txt")),
    fs.existsSync(join(sb.cwd, "uploads")) ? fs.readdirSync(join(sb.cwd, "uploads")).join(",") : "no uploads/",
  );
  check(
    "spec.pdf stored with the right bytes",
    fs.readFileSync(uploaded("spec.pdf"), "utf8") === PDF,
  );
  check(
    "the panel reports where the files went",
    /uploads\/spec\.pdf/.test(await panelText()) && /uploads\/notes\.txt/.test(await panelText()),
    (await panelText()).replace(/\s+/g, " ").slice(0, 200),
  );
  check("no prompt on a fresh name", dialogs.length === 0, dialogs.join(" | "));

  // --- round 2: the same name again must ASK before replacing ---
  fs.writeFileSync(specPath, "%PDF-1.4\nREPLACED\n%%EOF\n");
  await pick([specPath]);
  check("a replace prompt was shown", dialogs.length === 1, dialogs.join(" | "));
  check(
    "the prompt names the file it would replace",
    /spec\.pdf/.test(dialogs[0] ?? ""),
    dialogs[0] ?? "(none)",
  );
  check(
    "accepting the prompt replaced the file",
    fs.readFileSync(uploaded("spec.pdf"), "utf8").includes("REPLACED"),
  );

  // --- round 3: declining the prompt must leave the file alone ---
  chrome.ws.onmessage = (ev) => {
    prior(ev);
    const m = JSON.parse(ev.data);
    if (m.method === "Page.javascriptDialogOpening") {
      dialogs.push(m.params.message);
      void chrome.send("Page.handleJavaScriptDialog", { accept: false }, m.sessionId);
    }
  };
  fs.writeFileSync(specPath, "%PDF-1.4\nSHOULD-NOT-LAND\n%%EOF\n");
  await pick([specPath]);
  check("a second replace prompt was shown", dialogs.length === 2, String(dialogs.length));
  check(
    "declining left the previous contents untouched",
    fs.readFileSync(uploaded("spec.pdf"), "utf8").includes("REPLACED"),
    fs.readFileSync(uploaded("spec.pdf"), "utf8").replace(/\s+/g, " "),
  );

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the Details panel uploads files into uploads/ and asks before replacing");
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
