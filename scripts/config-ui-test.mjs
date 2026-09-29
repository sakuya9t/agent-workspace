// Headless-Chrome verification of the Config dialog (startup settings), driving
// the REAL managed-UI setup: a Vite dev server (with its `/__asm/config`
// endpoint) proxying to daemon A over the same data dir — so they share one
// startup.conf and must be listed as ONE host — plus a second daemon B the
// client is connected to directly. Proves: the host picker, the merge, editing +
// saving through both endpoints (the file on disk), write-only secrets,
// client-side validation, the role switch, the "not applied yet" hint, the
// stale-revision conflict + reload, unsaved-change protection, and the mobile
// entry point.
//
//   cd client && npm install               # Vite + deps (no build needed: dev server)
//   cargo build                            # asm-daemon
//   node scripts/config-ui-test.mjs        # sandboxed: 2 daemons + vite + chrome

import { spawn } from "node:child_process";
import { openSync, readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { createSandbox, checker, freePort, sleep, ROOT } from "./lib/testenv.mjs";

const { check, report } = checker();
const sbA = await createSandbox("asm-cfg-ui-a");
const sbB = await createSandbox("asm-cfg-ui-b");
let vite = null;

const parse = (text) =>
  Object.fromEntries(
    text
      .split("\n")
      .filter((l) => l && !l.startsWith("#") && l.includes("="))
      .map((l) => [l.slice(0, l.indexOf("=")), l.slice(l.indexOf("=") + 1)]),
  );
const confA = join(sbA.dataDir, "startup.conf");
const confB = join(sbB.dataDir, "startup.conf");

async function main() {
  const viteBin = join(ROOT, "client", "node_modules", ".bin", "vite");
  if (!existsSync(viteBin)) throw new Error(`missing ${viteBin} — run (cd client && npm install)`);

  await sbA.startDaemon();
  await sbB.startDaemon();

  const port = await freePort();
  const ui = `http://127.0.0.1:${port}`;
  const log = openSync(join(sbA.tmp, "vite.log"), "a");
  vite = spawn(viteBin, ["--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: join(ROOT, "client"),
    env: sbA.env({ ASM_DAEMON: sbA.http, ASM_CLIENT_HOST: "127.0.0.1", ASM_UI_MANAGED: "1" }),
    stdio: ["ignore", log, log],
  });
  let uiView = null;
  for (let i = 0; i < 100 && !uiView; i++) {
    try {
      const r = await fetch(`${ui}/__asm/config`);
      if (r.ok) uiView = await r.json();
    } catch {
      /* not up */
    }
    if (!uiView) await sleep(200);
  }
  check("vite serves /__asm/config for the UI host", uiView?.source === "ui", uiView?.path);
  check("…pointing at the same file as daemon A", uiView?.path === confA);

  const chrome = await sbA.startChrome();
  const page = await chrome.openPage(`${ui}/`);
  const { evalJs, waitFor, S } = page;
  await waitFor(`location.origin === ${JSON.stringify(ui)} && document.readyState === 'complete'`, 30000);
  // Connect the client to A (same-origin, via Vite's proxy) and B (direct).
  await evalJs(`localStorage.setItem("asm.daemons", JSON.stringify([
    { id: "local", baseUrl: "", token: null, label: "This machine", connected: true },
    { id: "b", baseUrl: ${JSON.stringify(sbB.http)}, token: null, label: "box-b", connected: true },
  ])); true`);
  const reload = async () => {
    await S("Page.reload", {});
    await sleep(300);
    await waitFor("document.readyState === 'complete'", 30000);
  };
  await reload();

  const helpers = `
    window.__btn = (scope, text) =>
      [...document.querySelectorAll(scope + " button")].find(b => b.textContent.trim() === text);
    window.__field = (key) =>
      [...document.querySelectorAll(".config-modal .config-field")]
        .find(f => f.querySelector(".config-key")?.textContent === key);
    window.__input = (key) => window.__field(key)?.querySelector("input, select");
    window.__set = (key, val) => {
      const el = window.__input(key);
      const proto = el.tagName === "SELECT" ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, "value").set.call(el, val);
      el.dispatchEvent(new Event(el.tagName === "SELECT" ? "change" : "input", { bubbles: true }));
    };
    window.__chips = () => [...document.querySelectorAll(".config-modal .config-host")];
    // Component switches: 0 daemon, 1 relay, 2 web UI.
    window.__comp = (i) => document.querySelectorAll(".config-modal .config-component")[i];
    window.__save = () => window.__btn(".config-modal", "Save");
    true`;
  const open = async () => {
    await evalJs(helpers);
    await evalJs("document.querySelector('.config-btn').click()");
    return waitFor("!!document.querySelector('.config-modal')");
  };

  check("config button in the top bar", await waitFor("!!document.querySelector('.topbar .config-btn')", 30000));
  check("dialog opens", await open());
  check(
    "two hosts: the UI server merged with daemon A, and daemon B",
    await waitFor("__chips().length === 2 && /serves this page · daemon/.test(__chips()[0].innerText) && /box-b/.test(__chips()[1].innerText)"),
    await evalJs("__chips().map(c => c.innerText.replace(/\\n/g, ' ')).join(' | ')"),
  );
  check(
    "no file yet → says so",
    await waitFor("/Nothing saved on this host yet/.test(document.querySelector('.config-modal').innerText)"),
  );
  check("Save starts disabled (nothing changed)", await evalJs("__save().disabled === true"));

  // ── edit the UI host (merged with A) ───────────────────────────────────────
  await evalJs("__set('ASM_NODE_LABEL', 'alpha')");
  check("an edit enables Save and tags the host unsaved", await waitFor("!__save().disabled && /unsaved/.test(__chips()[0].innerText)"));
  await evalJs("__save().click()");
  check("saved notice", await waitFor("/Saved on/.test(document.querySelector('.config-modal').innerText)"));
  check("file written on the UI host", existsSync(confA) && parse(readFileSync(confA, "utf8")).ASM_NODE_LABEL === "alpha");
  const aView = await sbA.api("/api/config");
  check("daemon A reads the same saved value", aView.values.ASM_NODE_LABEL === "alpha");

  // Client-side validation.
  await evalJs("__set('ASM_UI_PORT', '99999')");
  check(
    "invalid port → inline error, Save disabled",
    await waitFor("/Expected a port between 1 and 65535/.test(__field('ASM_UI_PORT').innerText) && __save().disabled"),
  );
  await evalJs("__set('ASM_UI_PORT', '5273')");

  // Unsaved changes are protected on close.
  await evalJs("__set('ASM_NODE_LABEL', 'alpha2')");
  await evalJs("window.confirm = () => false; __btn('.config-modal', 'Close').click(); true");
  check("close with unsaved edits asks first (declined → stays open)", await evalJs("!!document.querySelector('.config-modal')"));
  await evalJs("window.confirm = () => true; __btn('.config-modal', 'Close').click(); true");
  check("…accepted → closes", await waitFor("!document.querySelector('.config-modal')"));

  // Pending: the scripts applied something else than what is saved.
  writeFileSync(join(sbA.runDir, "startup.applied"), readFileSync(confA, "utf8").replace("ASM_NODE_LABEL=alpha", "ASM_NODE_LABEL=old"));
  check("reopens", await open());
  check(
    "not-applied hint names the pending setting",
    await waitFor("/not applied/.test(__chips()[0].innerText) && /Saved but not applied yet: Node label/.test(document.querySelector('.config-modal').innerText)"),
  );

  // ── edit daemon B ──────────────────────────────────────────────────────────
  await evalJs("__chips()[1].click()");
  check("switching host shows B's form", await waitFor("__chips()[1].classList.contains('on') && !!__input('ASM_BIND')"));
  check("B's listen address is its real bind", (await evalJs("__input('ASM_BIND').value")) === sbB.base);
  await evalJs("__set('ASM_RELAY_URL', 'ws://relay.example:4700')");
  check(
    "registering without a key is refused client-side",
    await waitFor("/Registering needs the relay's access key/.test(__field('ASM_RELAY_KEY').innerText) && __save().disabled"),
  );
  await evalJs("__set('ASM_RELAY_KEY', 'k1')");
  check("typing the key clears it", await waitFor("!__save().disabled"));
  await evalJs("__save().click()");
  await waitFor("/Saved on box-b/.test(document.querySelector('.config-modal').innerText)");
  const onB = existsSync(confB) ? parse(readFileSync(confB, "utf8")) : {};
  check("B's file has the registration + key", onB.ASM_RELAY_URL === "ws://relay.example:4700" && onB.ASM_RELAY_KEY === "k1");
  const bView = await sbB.api("/api/config");
  check("B never echoes the key", bView.values.ASM_RELAY_KEY === undefined && bView.secrets.ASM_RELAY_KEY === true);
  check(
    "the key field now reads as saved, not shown",
    await waitFor("__input('ASM_RELAY_KEY').value === '' && /saved/.test(__input('ASM_RELAY_KEY').placeholder)"),
  );

  // Components switch independently.
  check(
    "three component switches: daemon + UI on, relay off",
    await evalJs("[0,1,2].map(i => __comp(i).getAttribute('aria-checked')).join() === 'true,false,true'"),
  );
  await evalJs("__comp(0).click()");
  check(
    "daemon off: its fields go, the UI's proxy target appears, a warning is raised",
    await waitFor("!__input('ASM_BIND') && !!__input('ASM_UI_DAEMON') && /will not run a daemon/.test(document.querySelector('.config-modal').innerText)"),
  );
  await evalJs("__comp(1).click()");
  check(
    "relay on without a daemon: its fields appear; a key is required",
    await waitFor("!!__input('ASM_RELAY_BIND') && /needs at least one access key/.test(__field('ASM_RELAY_KEYS').innerText) && __save().disabled"),
  );
  await evalJs("__comp(1).click(); __comp(2).click(); true");
  check(
    "everything off is refused",
    await waitFor("/Turn on at least one/.test(document.querySelector('.config-modal').innerText) && __save().disabled"),
  );
  await evalJs("__comp(0).click(); __comp(2).click(); true");
  check("back to daemon + UI: no pending change left", await waitFor("!!__input('ASM_BIND') && __save().disabled"));

  // The enrollment token is a write-only daemon setting.
  await evalJs("__set('ASM_ENROLLMENT_TOKEN', 'short')");
  check("a short enrollment token is refused", await waitFor("/At least 16 characters/.test(__field('ASM_ENROLLMENT_TOKEN').innerText) && __save().disabled"));
  await evalJs("__set('ASM_ENROLLMENT_TOKEN', 'box-b-enroll-token-01')");
  await evalJs("__save().click()");
  await waitFor("/Saved on box-b/.test(document.querySelector('.config-modal').innerText)");
  check(
    "enrollment token saved on B, reported only as set",
    parse(readFileSync(confB, "utf8")).ASM_ENROLLMENT_TOKEN === "box-b-enroll-token-01" &&
      (await sbB.api("/api/config")).secrets.ASM_ENROLLMENT_TOKEN === true,
  );

  // Conflict: something else rewrites B's file while the dialog holds it.
  appendFileSync(confB, "# touched by the start script\n");
  await evalJs("__set('ASM_NODE_LABEL', 'beta')");
  await evalJs("__save().click()");
  check(
    "stale edit → conflict message + Reload",
    await waitFor("/changed since they were loaded/.test(document.querySelector('.config-modal').innerText) && !!__btn('.config-modal', 'Reload')"),
  );
  check("…and nothing was written", parse(readFileSync(confB, "utf8")).ASM_NODE_LABEL !== "beta");
  await evalJs("__btn('.config-modal', 'Reload').click()");
  check("reload discards the draft", await waitFor("__input('ASM_NODE_LABEL').value === '' && __save().disabled"));
  await evalJs("__set('ASM_NODE_LABEL', 'beta')");
  await evalJs("__save().click()");
  check(
    "save after reload succeeds",
    await waitFor("/Saved on box-b/.test(document.querySelector('.config-modal').innerText)") &&
      parse(readFileSync(confB, "utf8")).ASM_NODE_LABEL === "beta",
  );
  await evalJs("__btn('.config-modal', 'Close').click()");

  // ── mobile entry point ─────────────────────────────────────────────────────
  await S("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
  await reload();
  check("phone: config button in the session list header", await waitFor("!!document.querySelector('.sessions .panel-header .config-btn')", 30000));
  check(
    "…fully on screen",
    await evalJs("(() => { const r = document.querySelector('.sessions .panel-header .config-btn').getBoundingClientRect(); return r.left >= 0 && r.right <= window.innerWidth; })()"),
  );
  check("…and opens the dialog", await open());
  check(
    "dialog fits the phone width",
    await waitFor("document.querySelector('.config-modal').getBoundingClientRect().right <= window.innerWidth && __chips().length === 2"),
  );
}

try {
  await main();
} catch (e) {
  check(`unexpected error: ${e.message}`, false);
} finally {
  if (vite) {
    try {
      vite.kill("SIGKILL");
    } catch {
      /* gone */
    }
  }
  sbA.cleanup();
  sbB.cleanup();
}
process.exit(report("Config dialog: host picker, save, secrets, validation, conflict, mobile") ? 0 : 1);
