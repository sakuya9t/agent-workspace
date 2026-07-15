// Headless-Chrome verification that the two destructive session actions — STOP
// (kills the agent) and ARCHIVE (drops the session from history and deletes the
// branch/worktree it created) — are confirmed before they act.
//
//   cd client && npm run build              # once, to produce client/dist
//   node scripts/confirm-destructive-test.mjs
//
// Both halves are tested the same way, and the *dismiss* half is the one that
// matters: a confirm() nobody can say no to is just a slower button.
//
//   1. stop, dismissed    -> session must STILL be running
//   2. stop, accepted     -> session stops
//   3. archive, dismissed -> session must STILL be in history, not archived
//   4. archive, accepted  -> archived, and gone from history
//
// The dialog text is asserted too, not just its existence: rows carry no visible
// session id, so the prompt has to echo back what the row showed ("shell · cwd")
// or a mis-click gets confirmed as readily as the intended click.
//
// Layout note (same as archive-kickout-test): the workspace tree lists only LIVE
// sessions, so once stopped, a session moves to the History section — collapsed
// by default. Hence OPEN_HISTORY before the archive half.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-confirm");

const newSession = async () =>
  (
    await sb.api("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
    })
  ).session;
const statusOf = async (id) =>
  (await sb.api("/api/sessions")).sessions.find((s) => s.id === id)?.status;

const DESKTOP = { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false };
const POLL = 8000; // the client polls every 1.5s — allow several cycles

const OPEN_HISTORY = `(() => {
  const sec = document.querySelector('.history-section');
  if (!sec) return false;
  if (!sec.classList.contains('open')) document.querySelector('.history-header').click();
  return true;
})()`;
const HIST_ROW = ".history-list .session-row";

/** Click the row's action button whose tooltip matches `re` (buttons are icon-only). */
const clickAction = (rowSel, re) => `(() => {
  const r = document.querySelector('${rowSel}');
  if (!r) return 'no row';
  const b = [...r.querySelectorAll('button')].find((b) => ${re}.test(b.title || ''));
  if (!b) return 'no button';
  b.click();
  return 'clicked';
})()`;

/** Poll the daemon until `id` reaches `want` (the UI acts, the daemon follows). */
async function reaches(id, want, ms = 8000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if ((await statusOf(id)) === want) return true;
    await sleep(250);
  }
  return false;
}

async function main() {
  await sb.startAppDaemon();
  const chrome = await sb.startChrome();

  // Dialog pump. `answer` is what the next confirm() gets — flipping it is how
  // the dismiss cases are driven. Recording the text lets us assert on it, and
  // answering at all is mandatory: an open dialog freezes every Runtime.evaluate
  // in that renderer.
  let answer = true;
  const dialogs = [];
  const prev = chrome.ws.onmessage;
  chrome.ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.method === "Page.javascriptDialogOpening") {
      dialogs.push(m.params.message);
      chrome.send("Page.handleJavaScriptDialog", { accept: answer }, m.sessionId);
    }
    prev(ev);
  };
  const last = () => dialogs[dialogs.length - 1] ?? "";
  const firstLine = () => last().split("\n")[0];

  const page = await chrome.openPage(`${sb.http}/`);
  await page.S("Emulation.setDeviceMetricsOverride", DESKTOP);
  await page.S("Network.setCacheDisabled", { cacheDisabled: true });

  const a = await newSession();
  check("session running", a.status === "running", a.id.slice(0, 8));
  check("row rendered (live tree)", await page.waitFor("!!document.querySelector('.session-row')"));

  // ======================================================== 1. stop, dismissed
  answer = false;
  const c1 = await page.evalJs(clickAction(".session-row", /stop/i));
  check("stop clicked", c1 === "clicked", c1);
  await sleep(1500);
  check("stop prompted a confirm()", dialogs.length === 1, firstLine());
  check("stop prompt names the session", last().includes("shell") && last().includes("cwd"), firstLine());
  check("DISMISSED -> still running  <-- the point of the test", (await statusOf(a.id)) === "running");

  // ======================================================== 2. stop, accepted
  answer = true;
  const c2 = await page.evalJs(clickAction(".session-row", /stop/i));
  check("stop clicked again", c2 === "clicked", c2);
  check("ACCEPTED -> stopped", await reaches(a.id, "stopped"), await statusOf(a.id));

  // ==================================================== 3. archive, dismissed
  check("history opened", await page.waitFor(OPEN_HISTORY, POLL));
  check("stopped session is in history", await page.waitFor(`!!document.querySelector('${HIST_ROW}')`, POLL));

  answer = false;
  const before = dialogs.length;
  const c3 = await page.evalJs(clickAction(HIST_ROW, /archive/i));
  check("archive clicked", c3 === "clicked", c3);
  await sleep(1500);
  check("archive prompted a confirm()", dialogs.length === before + 1, firstLine());
  check(
    "archive prompt names the session and warns it is final",
    last().includes("shell") && /cannot be undone/i.test(last()),
    firstLine(),
  );
  check("DISMISSED -> not archived  <-- the point of the test", (await statusOf(a.id)) !== "archived");
  check("DISMISSED -> still listed in history", await page.evalJs(`!!document.querySelector('${HIST_ROW}')`));

  // ===================================================== 4. archive, accepted
  answer = true;
  const c4 = await page.evalJs(clickAction(HIST_ROW, /archive/i));
  check("archive clicked again", c4 === "clicked", c4);
  check("ACCEPTED -> archived on the daemon", await reaches(a.id, "archived"));
  check(
    "ACCEPTED -> gone from history",
    await page.waitFor(`!document.querySelector('${HIST_ROW}')`, POLL),
  );
}

let ok = false;
try {
  await main();
} catch (e) {
  check("no exception", false, String(e?.stack?.split("\n").slice(0, 2).join(" | ") ?? e));
} finally {
  ok = report("stop and archive both ask before they act");
  sb.cleanup();
}
process.exit(ok ? 0 : 1);
