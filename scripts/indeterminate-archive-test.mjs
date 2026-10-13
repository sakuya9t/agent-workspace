// Headless-Chrome regression test: a session left `indeterminate` by a holder
// crash can be archived from the UI.
//
//   cargo build && (cd client && npm run build)   # once
//   node scripts/indeterminate-archive-test.mjs   # sandboxed: asmux + daemon + chrome
//
// `indeterminate` used to be a dead end. Reconcile only revisits starting/running
// rows, archive refuses an indeterminate session (its process might still be
// running in the worktree), and the refusal said "Stop it first" — but stop was
// a no-op on it and the UI offers no stop button for a non-live row. One holder
// restart stranded six sessions in history this way. Now stopping one resolves
// it to `stopped`, and the Archive button on an indeterminate row confirms with
// the unknown-outcome warning, stops it, then archives it.
//
//   1. two worktree sessions; daemon down, holder killed, both restarted
//      -> both reconcile `indeterminate`
//   2. raw archive still refuses (the guard is intact)
//   3. raw stop resolves one to `stopped`, badge cleared
//   4. in-UI archive on the other: the indeterminate confirm, no error alert,
//      session archived, its worktree reclaimed
//
// Without the fix, 3 stays `indeterminate` and 4 raises the daemon's refusal as
// an alert.

import { existsSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-indet");
const SIDECAR = { ASM_BACKEND: "sidecar", ASM_ASMUX_AUTOSPAWN: "0" };
const POLL = 10000; // the client polls every 1.5s — allow several cycles

/** A throwaway git repo with one commit — enough for worktree isolation. */
function makeRepo(dir) {
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: "t",
    GIT_AUTHOR_EMAIL: "t@test",
    GIT_COMMITTER_NAME: "t",
    GIT_COMMITTER_EMAIL: "t@test",
  };
  execFileSync("git", ["init", "-q", "-b", "main"], { cwd: dir, env });
  execFileSync("git", ["commit", "-q", "--allow-empty", "-m", "init"], { cwd: dir, env });
}

const sessionOf = async (id) => (await sb.api("/api/sessions")).sessions.find((s) => s.id === id);

async function main() {
  makeRepo(sb.cwd);
  await sb.startAsmux();
  await sb.startAppDaemon("daemon1", SIDECAR);

  const { workspace } = await sb.api("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "repo", root_path: sb.cwd }),
  });
  const mk = async () =>
    (
      await sb.api("/api/sessions", {
        method: "POST",
        body: JSON.stringify({ agent_plugin_id: "shell", workspace_id: workspace.id }),
      })
    ).session;
  const viaApi = await mk();
  const viaUi = await mk();
  const uiInst = (await sb.api(`/api/sessions/${viaUi.id}/workspace`)).instance;
  check("UI session has a worktree", uiInst?.isolation === "worktree" && existsSync(uiInst.path));

  // ============================================ 1: holder lost across a restart
  sb.stop("daemon1");
  await sb.waitExit("daemon1");
  sb.stop("asmux");
  await sb.waitExit("asmux");
  rmSync(sb.socket, { force: true });
  await sb.startAsmux();
  await sb.startAppDaemon("daemon2", SIDECAR);
  for (const s of [viaApi, viaUi]) {
    const st = (await sessionOf(s.id))?.status;
    check(`${s.id.slice(0, 8)} reconciled indeterminate`, st === "indeterminate", st);
  }

  // ============================================ 2: the archive guard still holds
  const res = await fetch(`${sb.http}/api/sessions/${viaUi.id}/archive?force=true`, {
    method: "POST",
  });
  const body = await res.text();
  check("raw archive refuses indeterminate", !res.ok && /outcome is unknown/.test(body), body);
  check("worktree untouched by the refusal", existsSync(uiInst.path));

  // ============================================ 3: stop resolves it
  const stopped = (await sb.api(`/api/sessions/${viaApi.id}/stop`, { method: "POST" })).session;
  check("stop -> stopped", stopped.status === "stopped", stopped.status);
  check("stop -> badge cleared", stopped.attention_state === "none", stopped.attention_state);

  // ============================================ 4: archive from the UI
  const chrome = await sb.startChrome();
  const dialogs = [];
  const prev = chrome.ws.onmessage;
  chrome.ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.method === "Page.javascriptDialogOpening") {
      dialogs.push({ type: m.params.type, message: m.params.message });
      chrome.send("Page.handleJavaScriptDialog", { accept: true }, m.sessionId);
    }
    prev(ev);
  };
  const page = await chrome.openPage(`${sb.http}/`);
  await page.S("Network.setCacheDisabled", { cacheDisabled: true });
  await page.S("Emulation.setDeviceMetricsOverride", {
    width: 1400,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await page.S("Page.reload", { ignoreCache: true });

  // History is collapsed by default; rows carry no id, so pick the one whose
  // ended-status reads "indeterminate" (the other is now "finished").
  const opened = await page.waitFor(`(() => {
    const sec = document.querySelector('.history-section');
    if (!sec) return false;
    if (!sec.classList.contains('open')) document.querySelector('.history-header').click();
    return !!document.querySelector('.history-list .session-row');
  })()`);
  check("history open", opened);
  const ROW = `[...document.querySelectorAll('.history-list .session-row')]
    .find(r => r.querySelector('.ended-status')?.textContent.trim() === 'indeterminate')`;
  check("indeterminate row rendered", await page.waitFor(`!!(${ROW})`));
  const clicked = await page.evalJs(`(() => {
    const r = ${ROW};
    const b = r && [...r.querySelectorAll('button')].find(b => /archive/i.test(b.title || ''));
    if (!b) return 'no archive button';
    b.click();
    return 'clicked';
  })()`);
  check("archive button clicked", clicked === "clicked", clicked);

  let st;
  for (let t0 = Date.now(); Date.now() - t0 < POLL; await sleep(300)) {
    st = (await sessionOf(viaUi.id))?.status;
    if (st === "archived") break;
  }
  check("UI session archived", st === "archived", st);
  check("its worktree reclaimed", !existsSync(uiInst.path), uiInst.path);
  check(
    "confirm named the unknown outcome",
    dialogs[0]?.type === "confirm" && /outcome is unknown/.test(dialogs[0]?.message ?? ""),
    JSON.stringify(dialogs[0]),
  );
  const alerts = dialogs.filter((d) => d.type === "alert");
  check("no error alert", alerts.length === 0, JSON.stringify(alerts));
}

let ok = false;
try {
  await main();
} catch (e) {
  check("no exception", false, String(e?.stack?.split("\n").slice(0, 2).join(" | ") ?? e));
} finally {
  ok = report("an indeterminate session can be archived");
  sb.cleanup();
}
process.exit(ok ? 0 : 1);
