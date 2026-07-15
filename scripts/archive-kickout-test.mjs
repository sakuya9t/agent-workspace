// Headless-Chrome verification that archiving a session kicks the client out of
// that session's conversation view — the same way a takeover does.
//
// Takeover is *pushed* (WS close 4001, see Terminal.tsx). Archive has no push
// channel at all: the daemon tells nobody, and clients only notice because they
// poll every 1.5s. So the kick-out hangs off the polled status rather than the
// archive click — which is what makes it fire on EVERY client viewing the
// session, not just the one that pressed the button. Case 2 is that case.
//
//   cd client && npm run build            # once, to produce client/dist
//   node scripts/archive-kickout-test.mjs # sandboxed: daemon + chrome + sessions
//
//   1. desktop: STOP  -> must STAY on the session   (negative control)
//   2. desktop: archive by ANOTHER client (raw API) -> must leave the session
//   3. mobile:  archive -> back to home AND the #s= deep link is tidied
//   4. desktop: archive via the in-UI button        -> must leave the session
//
// Case 1 is what stops an over-broad fix: keying the kick-out on "not live"
// instead of "archived" would strand you out of *stopped* sessions, which you
// must still be able to open and read. It passes with and without the fix; the
// six kick-out assertions all fail without it.
//
// Layout notes, learned the hard way: the workspace tree lists only LIVE
// sessions, so a stopped one moves into the History section — which is
// COLLAPSED by default (`historyOpen = useState(false)`). Rows carry no session
// id in their text, so each case keeps exactly one session in History and takes
// `.history-list .session-row`[0]. Archived sessions leave History entirely, and
// the section unmounts when empty — hence `.mobile-home-body`, not
// `.history-section`, as the "we're back on the list" marker.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-archive");

const newSession = async () =>
  (
    await sb.api("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
    })
  ).session;
const stop = (id) => sb.api(`/api/sessions/${id}/stop`, { method: "POST" });
const archive = (id) => sb.api(`/api/sessions/${id}/archive?force=true`, { method: "POST" });
const statusOf = async (id) =>
  (await sb.api("/api/sessions")).sessions.find((s) => s.id === id)?.status;

const DESKTOP = { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false };
const PHONE = { width: 390, height: 844, deviceScaleFactor: 2, mobile: true };
const POLL = 10000; // the client polls every 1.5s — allow several cycles

async function openShell(chrome, metrics) {
  const page = await chrome.openPage(`${sb.http}/`);
  await page.S("Emulation.setDeviceMetricsOverride", metrics);
  await page.S("Network.setCacheDisabled", { cacheDisabled: true });
  return page;
}

const OPEN_HISTORY = `(() => {
  const sec = document.querySelector('.history-section');
  if (!sec) return false;
  if (!sec.classList.contains('open')) document.querySelector('.history-header').click();
  return true;
})()`;
const HIST_ROW = ".history-list .session-row";
const XTERM = "!!document.querySelector('.xterm')";
const NO_XTERM = "!document.querySelector('.xterm')";

/** Expand History and open the single session sitting in it. */
async function openHistorySession(page) {
  if (!(await page.waitFor(OPEN_HISTORY))) return false;
  if (!(await page.waitFor(`!!document.querySelector('${HIST_ROW}')`))) return false;
  await page.evalJs(`document.querySelector('${HIST_ROW}').click()`);
  return page.waitFor(XTERM);
}

async function main() {
  await sb.startAppDaemon();
  const chrome = await sb.startChrome();

  // Auto-accept confirm() — the archive button prompts when the daemon answers
  // 409/needs-force, and an unanswered dialog freezes every Runtime.evaluate.
  const prev = chrome.ws.onmessage;
  chrome.ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.method === "Page.javascriptDialogOpening") {
      chrome.send("Page.handleJavaScriptDialog", { accept: true }, m.sessionId);
    }
    prev(ev);
  };

  // ================================================ 1 + 2: desktop
  const page = await openShell(chrome, DESKTOP);
  const a = await newSession();
  check("A running", a.status === "running", a.id.slice(0, 8));

  check("A row rendered (live tree)", await page.waitFor("!!document.querySelector('.session-row')"));
  await page.evalJs("document.querySelector('.session-row').click()");
  check("desktop: terminal open for A", await page.waitFor(XTERM));

  // --- 1. stop is NOT a kick-out (negative control) ---
  await stop(a.id);
  await sleep(POLL);
  const st = await statusOf(a.id);
  check("A terminal after stop", st !== "running" && st !== "archived", st);
  check("desktop: STOPPED session STAYS open  <-- negative control", await page.evalJs(XTERM));

  // --- 2. archive from another client kicks out ---
  await archive(a.id);
  check("A archived on the daemon", (await statusOf(a.id)) === "archived");
  check("desktop: archived -> terminal view gone", await page.waitFor(NO_XTERM, POLL));
  check(
    "desktop: archived -> empty panel shown",
    await page.waitFor("!!document.querySelector('.empty.big')", POLL),
  );

  // ================================================ 3: mobile
  const b = await newSession();
  await stop(b.id);
  await sleep(1500);
  const mob = await openShell(chrome, PHONE);
  check("mobile: history session opened", await openHistorySession(mob));
  const hashBefore = await mob.evalJs("location.hash");
  check("mobile: deep-link hash set", hashBefore.startsWith("#s="), hashBefore);

  await archive(b.id);
  check("mobile: archived -> left the terminal screen", await mob.waitFor(NO_XTERM, POLL));
  check(
    "mobile: archived -> back on the home screen",
    await mob.waitFor("!!document.querySelector('.mobile-home-body')", POLL),
  );
  check("mobile: deep-link hash tidied", (await mob.evalJs("location.hash")) === "");

  // ================================================ 4: in-UI archive button
  const c = await newSession();
  await stop(c.id);
  await sleep(1500);
  const p2 = await openShell(chrome, DESKTOP);
  check("in-UI: C opened from history", await openHistorySession(p2));

  const clicked = await p2.evalJs(`(() => {
    const r = document.querySelector('${HIST_ROW}');
    if (!r) return 'no row';
    const b = [...r.querySelectorAll('button')]
      .find(b => /archive/i.test((b.title || '') + ' ' + b.textContent));
    if (!b) return 'no archive button';
    b.click();
    return 'clicked';
  })()`);
  check("in-UI: archive button clicked", clicked === "clicked", clicked);
  check("in-UI: archived -> terminal view gone", await p2.waitFor(NO_XTERM, POLL));
  check("in-UI: C archived on the daemon", (await statusOf(c.id)) === "archived");
}

let ok = false;
try {
  await main();
} catch (e) {
  check("no exception", false, String(e?.stack?.split("\n").slice(0, 2).join(" | ") ?? e));
} finally {
  ok = report("archive kicks the client out of the session view");
  sb.cleanup();
}
process.exit(ok ? 0 : 1);
