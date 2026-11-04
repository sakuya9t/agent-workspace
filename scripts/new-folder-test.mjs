// Verifies the DirectoryPicker "+ New folder" button end-to-end:
//   API: POST /api/fs/mkdir creates a dir, rejects path-separator names & dupes.
//   UI:  new-workspace dialog → Browse → + New folder → name → Create
//        → folder exists on disk, is listed AND selected, "Use this folder"
//        carries it back into the workspace dialog. Invalid name shows an error.
import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, checker } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-mkdir");

async function main() {
  await sb.startAppDaemon();

  // --- API round: deterministic daemon behaviour ---
  const made = await sb.api("/api/fs/mkdir", {
    method: "POST",
    body: JSON.stringify({ parent: sb.cwd, name: "api-made" }),
  });
  check("mkdir returns the new path", made.path === join(sb.cwd, "api-made"), made.path);
  check("directory exists on disk", fs.statSync(made.path).isDirectory());

  const rejects = async (body, needle) => {
    try {
      await sb.api("/api/fs/mkdir", { method: "POST", body: JSON.stringify(body) });
      return false;
    } catch (e) {
      return e.message.includes("400") && e.message.includes(needle);
    }
  };
  check("rejects a/b (separator)", await rejects({ parent: sb.cwd, name: "a/b" }, "invalid folder name"));
  check("rejects .. traversal", await rejects({ parent: sb.cwd, name: ".." }, "invalid folder name"));
  check("rejects empty name", await rejects({ parent: sb.cwd, name: "  " }, "empty"));
  check("rejects duplicate", await rejects({ parent: sb.cwd, name: "api-made" }, "already exists"));
  check("rejects missing parent", await rejects({ parent: join(sb.cwd, "nope"), name: "x" }, "cannot resolve"));

  // --- UI round ---
  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { evalJs, waitFor } = page;

  // Page-side helpers: click a button by trimmed text, set a controlled input.
  await evalJs(`
    window.__btn = (scope, text) =>
      [...document.querySelectorAll(scope + " button")].find(b => b.textContent.trim() === text);
    window.__set = (el, val) => {
      const s = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      s.call(el, val); el.dispatchEvent(new Event("input", { bubbles: true }));
    };
    true`);

  check("daemon row rendered", await waitFor("!!document.querySelector('.tree-add')"));
  await evalJs("document.querySelector('.tree-add').click()");
  check("new-workspace dialog opened", await waitFor("!!document.querySelector('.path-row .btn')"));

  const openPicker = async () => {
    await evalJs("document.querySelector('.path-row .btn').click()");
    return waitFor("!!document.querySelector('.picker-modal')");
  };
  check("directory picker opened", await openPicker());

  // Navigate the picker to the sandbox cwd.
  await evalJs(`__set(document.querySelector('.picker-modal .picker-path-row input'), ${JSON.stringify(sb.cwd)}); __btn('.picker-modal', 'Go').click()`);
  check(
    "picker listed the sandbox dir",
    await waitFor(`[...document.querySelectorAll('.picker-modal .picker-entry')].some(e => e.textContent.includes('api-made'))`),
  );

  // + New folder → type a name → Create.
  check("new-folder button present", await evalJs("!!__btn('.picker-modal', '+ New folder')"));
  await evalJs("__btn('.picker-modal', '+ New folder').click()");
  check(
    "inline name row appeared (autofocused)",
    await waitFor(`document.activeElement?.placeholder === 'folder name'`),
  );
  await evalJs(`__set(document.querySelector('.picker-modal input[placeholder="folder name"]'), 'made-in-ui'); __btn('.picker-modal', 'Create').click()`);

  check(
    "new folder listed and selected",
    await waitFor(`document.querySelector('.picker-modal .picker-entry.selected')?.textContent.includes('made-in-ui')`),
  );
  const uiDir = join(sb.cwd, "made-in-ui");
  check("UI-created folder exists on disk", fs.existsSync(uiDir) && fs.statSync(uiDir).isDirectory());
  check(
    "path box shows the new folder",
    (await evalJs("document.querySelector('.picker-modal .picker-path-row input').value")) === uiDir,
  );
  check(
    "name row dismissed after create",
    await evalJs(`!document.querySelector('.picker-modal input[placeholder="folder name"]')`),
  );

  // Confirm: "Use this folder" carries the new dir into the workspace dialog.
  await evalJs("document.querySelector('.picker-modal .modal-actions .btn.primary').click()");
  check("picker closed on pick", await waitFor("!document.querySelector('.picker-modal')"));
  check(
    "workspace dialog got the new path",
    (await evalJs("document.querySelector('.path-row input').value")) === uiDir,
  );

  // Invalid-name round: the daemon's 400 must surface in the picker.
  check("picker reopened", await openPicker());
  // Button is disabled until the listing loads — wait for it to enable.
  check(
    "new-folder button enabled after load",
    await waitFor(`__btn('.picker-modal', '+ New folder')?.disabled === false`),
  );
  await evalJs("__btn('.picker-modal', '+ New folder').click()");
  check(
    "name row reopened",
    await waitFor(`!!document.querySelector('.picker-modal input[placeholder="folder name"]')`),
  );
  await evalJs(`__set(document.querySelector('.picker-modal input[placeholder="folder name"]'), 'a/b'); __btn('.picker-modal', 'Create').click()`);
  check(
    "invalid name surfaces an error",
    await waitFor(`document.querySelector('.picker-modal .error')?.textContent.includes('invalid folder name')`),
    await evalJs("document.querySelector('.picker-modal .error')?.textContent ?? ''"),
  );
  check("no bogus dirs created", !fs.existsSync(join(sb.cwd, "a")) && !fs.existsSync(join(uiDir, "a")));

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the + New folder button creates, selects, and validates via the daemon");
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
