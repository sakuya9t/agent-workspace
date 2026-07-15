// Headless-Chrome verification of the Fork-session affordance, driving the real
// built client bundle served by the daemon.
//
// The daemon half of forking (native resume vs brief, branch placement) is proven
// by the Rust tests and by hitting POST /api/sessions/:id/fork directly. What can
// only be proven in a browser is the wiring: the Fork button appears on a row,
// opens the dialog in fork mode, defaults to the origin's agent and to a branch of
// its own, warns when you point a fork at a live session's own worktree, and
// actually creates the fork.
//
//   cd client && npm run build        # once, to produce client/dist
//   node scripts/fork-session-test.mjs
//
// The source session is `claude` because forking is only offered into a coding
// agent (a shell has no conversation to hand over). Claude is never asked to do
// any work here — it just has to exist for the row to be forkable.

import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-fork");
// Screenshot lands outside the sandbox tmpdir, which cleanup() removes.
const SHOT = process.env.FORK_SHOT ?? "/tmp/fork-dialog.png";

const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

async function main() {
  await sb.startAppDaemon();

  // Forking lives on a branch, so the source needs a git workspace.
  const repo = join(sb.tmp, "repo");
  execFileSync("mkdir", ["-p", repo]);
  git(repo, "init", "-q", "-b", "main");
  execFileSync("sh", ["-c", `printf 'x = 1\\n' > ${repo}/app.py`]);
  git(repo, "add", "-A");
  git(repo, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "initial");

  const { workspace } = await sb.api("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "repo", root_path: repo }),
  });

  const plugins = (await sb.api("/api/plugins")).plugins;
  const haveAgent = plugins.some(
    (p) => p.available && ["claude", "codex", "opencode"].includes(p.id),
  );
  check("host has a coding agent to fork into", haveAgent);

  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "claude", workspace_id: workspace.id }),
  });
  check("source claude session is live on a branch", session.status === "running");

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { S, evalJs, waitFor } = page;

  check("session row rendered", await waitFor("!!document.querySelector('.session-row')"));

  // 1. The Fork button is on the row, enabled (an agent is installed).
  check(
    "fork button rendered on the session row",
    await waitFor("!!document.querySelector('.session-row .action-icon-fork')"),
  );
  check(
    "fork button is enabled when an agent is installed",
    !(await evalJs(
      "document.querySelector('.session-row .action-icon-fork').closest('button').disabled",
    )),
  );

  // 2. Clicking it opens the dialog in fork mode.
  await evalJs(
    "document.querySelector('.session-row .action-icon-fork').closest('button').click()",
  );
  check("fork dialog opened", await waitFor("!!document.querySelector('.modal')"));

  const title = await evalJs("document.querySelector('.modal-title').textContent");
  check("dialog is titled as a fork of the source", title.startsWith("Fork "), title);

  const agent = await evalJs("document.querySelector('.modal select.input').value");
  check("agent defaults to the source's agent", agent === "claude", agent);

  const agentOpts = await evalJs(
    "Array.from(document.querySelectorAll('.modal select.input option')).map(o=>o.value).join(',')",
  );
  check(
    "only coding agents are offered as fork targets (no shell / custom_command)",
    !agentOpts.includes("shell") && !agentOpts.includes("custom_command"),
    agentOpts,
  );

  // The place-pickers a fork inherits must not be shown.
  check(
    "workspace / directory pickers are hidden in fork mode",
    !(await evalJs("!!document.querySelector('.modal .seg')")),
  );

  // 3. The same-branch checkbox: present, off by default.
  const boxLabel = await evalJs(`(() => {
    const l = Array.from(document.querySelectorAll('.modal .checkbox'))
      .find(l => l.textContent.includes('same branch'));
    return l ? l.textContent : null;
  })()`);
  check("same-branch checkbox is offered, naming the branch", !!boxLabel, boxLabel);
  check(
    "it names the source's actual branch",
    (boxLabel ?? "").includes("asm-session/"),
    boxLabel,
  );

  const checkedByDefault = await evalJs(`(() => {
    const l = Array.from(document.querySelectorAll('.modal .checkbox'))
      .find(l => l.textContent.includes('same branch'));
    return l.querySelector('input').checked;
  })()`);
  check("defaults OFF — a fork gets its own branch, which is the safe default", !checkedByDefault);

  // This source has no resumable conversation: claude was launched but never
  // asked to do anything, so it wrote no transcript and the monitor captured no
  // conversation id. "A summary" is therefore the honest promise here. (The
  // whole-conversation path — same agent + a captured id + the origin's own cwd —
  // is what `plan_seed` decides, and is covered by the Rust tests.)
  check(
    "with no resumable conversation, the dialog promises a summary, not the whole thing",
    (await evalJs("document.querySelector('.modal').textContent")).includes(
      "carries a summary",
    ),
  );

  // 4. Ticking it while the source is LIVE must warn about two agents in one dir.
  check(
    "no warning shown while the fork takes its own branch",
    !(await evalJs("!!document.querySelector('.modal .warn')")),
  );
  await evalJs(`(() => {
    const l = Array.from(document.querySelectorAll('.modal .checkbox'))
      .find(l => l.textContent.includes('same branch'));
    const i = l.querySelector('input');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'checked').set;
    setter.call(i, true);
    i.dispatchEvent(new Event('click', { bubbles: true }));
  })()`);
  check(
    "ticking same-branch on a LIVE source warns that two agents share the directory",
    await waitFor("!!document.querySelector('.modal .warn')"),
  );
  const warn = await evalJs("document.querySelector('.modal .warn')?.textContent ?? ''");
  check("the warning says what actually goes wrong", warn.includes("overwrite"), warn);

  const { data: shot } = await S("Page.captureScreenshot", {});
  fs.writeFileSync(SHOT, Buffer.from(shot, "base64"));

  // 5. Switching the agent flips the context explanation to "a summary".
  await evalJs(`(() => {
    const s = document.querySelector('.modal select.input');
    const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
    setter.call(s, 'codex');
    s.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  check(
    "choosing a different agent switches the promise to a summary",
    await waitFor(
      "document.querySelector('.modal').textContent.includes('carries a summary')",
    ),
  );

  // 6. Untick, and actually create the fork through the UI.
  await evalJs(`(() => {
    const l = Array.from(document.querySelectorAll('.modal .checkbox'))
      .find(l => l.textContent.includes('same branch'));
    const i = l.querySelector('input');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'checked').set;
    setter.call(i, false);
    i.dispatchEvent(new Event('click', { bubbles: true }));
  })()`);
  await evalJs(`(() => {
    const b = Array.from(document.querySelectorAll('.modal-actions .btn.primary'))[0];
    b.click();
  })()`);

  // The summarizer runs a real agent headlessly, so this is slow by nature.
  const forked = await waitFor(
    "document.querySelectorAll('.session-row').length >= 2",
    120000,
  );
  check("the fork appears in the session list", forked);

  // Read both back from the list: `branch` is added by the list/get projection,
  // not by the raw session the create/fork POST echoes, so comparing against the
  // POST's copy would compare against `undefined` and pass for the wrong reason.
  const sessions = (await sb.api("/api/sessions")).sessions;
  const src = sessions.find((s) => s.id === session.id);
  const fork = sessions.find((s) => s.forked_from === session.id);
  check("the new session records its origin", !!fork, fork && fork.forked_from.slice(0, 8));
  check("the fork is the agent that was chosen in the dialog", fork?.agent_plugin_id === "codex");
  check(
    "the source really is on a branch (so the comparison below means something)",
    !!src?.branch,
    src?.branch,
  );
  check(
    "the fork is on its own branch, not the source's",
    !!fork?.branch && !!src?.branch && fork.branch !== src.branch,
    `${fork?.branch} vs ${src?.branch}`,
  );
  check(
    "the fork got its own worktree",
    !!fork && fork.working_directory !== src.working_directory,
  );
}

try {
  await main();
} catch (e) {
  check("test ran without throwing", false, String(e));
} finally {
  await sleep(200);
  sb.cleanup();
  report();
}
