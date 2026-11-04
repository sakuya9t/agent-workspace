// Verification of the stop protection: a session whose agent is WORKING or
// BLOCKED has a turn in flight, and stopping it there throws that work away — so
// the daemon refuses an unforced stop (409) and the UI gates the button behind a
// "force stop" checkbox.
//
//   cd client && npm run build           # once, to produce client/dist
//   node scripts/stop-protection-test.mjs
//
// Two halves:
//
//   A. HTTP — POST /stop is 409 while working and while blocked, and leaves the
//      session running; `?force=true` ends it. An idle session stops with no
//      force at all, which is what keeps the guard from being a nuisance.
//   B. UI  — the stop dialog on a working session shows the warning and the
//      checkbox, with Stop DEAD until it is ticked; ticking it stops the
//      session. The idle-session dialog (no checkbox, one click) is covered by
//      confirm-destructive-test.mjs.
//
// Attention state is *derived* — the monitor classifies real output — so the
// fixtures earn their states rather than being written into the DB: `shell`
// opts out of attention tracking entirely, so these run under `custom_command`
// (which tracks it, via the default tail classifier) with a bash one-liner as
// the "agent". A loop that keeps printing reads as working; a line ending in
// "(y/n)" reads as blocked; printing once and then sleeping settles to idle.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-stopguard");

/** A fake agent: real session, real monitor, no agent binary. */
const fakeAgent = async (script) =>
  (
    await sb.api("/api/sessions", {
      method: "POST",
      body: JSON.stringify({
        agent_plugin_id: "custom_command",
        cwd: sb.cwd,
        command: "/bin/bash",
        args: ["-c", script],
        approve_custom: true,
      }),
    })
  ).session;

const WORKING = "while true; do printf 'building the thing…\\n'; sleep 0.4; done";
const BLOCKED = "printf 'Delete 12 files? Do you want to proceed? (y/n) '; sleep 600";
// The leading sleep is load-bearing: output emitted before the monitor attaches
// lands in the attach *snapshot*, not the chunk stream, so an agent that prints
// instantly and then goes quiet can be classified never at all — and `idle` is
// only ever reached by settling down from `activity`.
const QUIET = "sleep 1; printf 'ready\\n'; sleep 600";

const sessionOf = async (id) => (await sb.api("/api/sessions")).sessions.find((s) => s.id === id);
const statusOf = async (id) => (await sessionOf(id))?.status;

/** Raw stop, so a refusal can be inspected instead of thrown. */
const stop = async (id, force = false) => {
  const res = await fetch(`${sb.http}/api/sessions/${id}/stop${force ? "?force=true" : ""}`, {
    method: "POST",
  });
  return { status: res.status, body: await res.text() };
};

/** Poll until the monitor has classified the session the way the test needs. */
async function settles(id, want, ms = 15000) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    last = (await sessionOf(id))?.attention_state;
    if (last === want) return true;
    await sleep(250);
  }
  return false;
}

async function reaches(id, want, ms = 8000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if ((await statusOf(id)) === want) return true;
    await sleep(250);
  }
  return false;
}

const MODAL = ".modal-backdrop .modal";
const CHECKBOX = `${MODAL} input[type=checkbox]`;
const DESKTOP = { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false };
const POLL = 8000; // the client polls every 1.5s — allow several cycles

/** The stop dialog's confirm button (the one that is not Cancel). */
const STOP_BTN = `[...document.querySelectorAll('${MODAL} .modal-actions button')].find((b) => !/cancel/i.test(b.textContent || ''))`;

/** Click the row's action button whose tooltip matches `re` (buttons are icon-only). */
const clickAction = (rowSel, re) => `(() => {
  const r = document.querySelector('${rowSel}');
  if (!r) return 'no row';
  const b = [...r.querySelectorAll('button')].find((b) => ${re}.test(b.title || ''));
  if (!b) return 'no button';
  b.click();
  return 'clicked';
})()`;

async function main() {
  await sb.startAppDaemon();

  // ============================================================ A. HTTP guard
  for (const [label, script, want] of [
    ["working", WORKING, "activity"],
    ["blocked", BLOCKED, "approval_needed"],
  ]) {
    const s = await fakeAgent(script);
    check(`${label} session reaches "${want}"`, await settles(s.id, want), (await sessionOf(s.id))?.attention_state);

    const refused = await stop(s.id);
    check(`${label} -> 409  <-- the point of the test`, refused.status === 409, `${refused.status} ${refused.body}`);
    check(`${label} -> refusal says how to override`, /force stop/i.test(refused.body), refused.body);

    // A refused stop must be a no-op, not a half-kill: still running, still
    // carrying the same state, still attachable.
    const after = await sessionOf(s.id);
    check(
      `${label} -> session left untouched`,
      after.status === "running" && after.attention_state === want,
      `${after.status}/${after.attention_state}`,
    );

    const forced = await stop(s.id, true);
    check(`${label} + force -> 200`, forced.status === 200, `${forced.status} ${forced.body.slice(0, 80)}`);
    check(`${label} + force -> stopped`, await reaches(s.id, "stopped"), await statusOf(s.id));
  }

  // Idle is unprotected: nothing is in flight, so no ceremony.
  const idle = await fakeAgent(QUIET);
  check("quiet session settles to idle", await settles(idle.id, "idle"), (await sessionOf(idle.id))?.attention_state);
  const plain = await stop(idle.id);
  check("idle -> stops unforced", plain.status === 200, `${plain.status} ${plain.body.slice(0, 80)}`);
  check("idle -> stopped", await reaches(idle.id, "stopped"));

  // ============================================================== B. UI gate
  // Only this one is live now, so the tree holds exactly one row.
  const live = await fakeAgent(WORKING);
  check("UI fixture reaches working", await settles(live.id, "activity"));

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  await page.S("Emulation.setDeviceMetricsOverride", DESKTOP);
  await page.S("Network.setCacheDisabled", { cacheDisabled: true }); // else a stale bundle runs the OLD code

  check(
    "row shows the working badge",
    await page.waitFor(
      `!!document.querySelector('.session-row .attn-badge') && /working/i.test(document.querySelector('.session-row').innerText)`,
      POLL,
    ),
    await page.evalJs("document.querySelector('.session-row')?.innerText?.replace(/\\n/g,' | ')"),
  );

  const clicked = await page.evalJs(clickAction(".session-row", /stop/i));
  check("stop clicked", clicked === "clicked", clicked);
  check("stop dialog opened", await page.waitFor(`!!document.querySelector('${MODAL}')`));

  const text = await page.evalJs(`document.querySelector('${MODAL}').innerText`);
  check("dialog warns the turn is in flight", /working/i.test(text), text.replace(/\n/g, " | "));
  check("force checkbox offered", await page.evalJs(`!!document.querySelector('${CHECKBOX}')`));
  check(
    "Stop is DEAD until force is ticked  <-- the point of the test",
    (await page.evalJs(`${STOP_BTN}?.disabled`)) === true,
  );
  await sleep(1500);
  check("session still running while the dialog sits open", (await statusOf(live.id)) === "running");

  // Tick the box the way a user would (React listens for the change event).
  check(
    "checkbox ticks",
    await page.evalJs(`(() => {
      const c = document.querySelector('${CHECKBOX}');
      c.click();
      return c.checked;
    })()`),
  );
  check("Stop is live once force is ticked", (await page.evalJs(`${STOP_BTN}?.disabled`)) === false);

  await page.evalJs(`${STOP_BTN}.click()`);
  check("forced stop from the UI -> stopped", await reaches(live.id, "stopped"), await statusOf(live.id));
  check("dialog closes on success", await page.waitFor(`!document.querySelector('${MODAL}')`, POLL));

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
}

let ok = false;
try {
  await main();
} catch (e) {
  check("no exception", false, String(e?.stack?.split("\n").slice(0, 3).join(" | ") ?? e));
} finally {
  ok = report("working/blocked sessions refuse an unforced stop");
  sb.cleanup();
}
process.exit(ok ? 0 : 1);
