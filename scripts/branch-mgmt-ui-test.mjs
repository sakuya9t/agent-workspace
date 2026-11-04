// Headless-Chrome verification of the workspace branch-management UI, driving
// the real built client bundle served by the daemon. Companion to
// `branch-mgmt-test.mjs` (which proves the daemon endpoints deterministically);
// this proves the browser wiring: the (i) icon on a Git workspace row opens the
// BranchManagerDialog and it renders one row per branch with the base + merged-
// nowhere info and the merge/rebase/delete controls.
//
//   cd client && npm run build             # once, to produce client/dist
//   node scripts/branch-mgmt-ui-test.mjs   # sandboxed: daemon + chrome + repo
//
// The app is served same-origin on loopback (baseUrl="" ⇒ loopback trust ⇒ no
// token). The host→workspace tree is expanded by default, so the workspace row's
// trailing controls are directly present. Node 18+ (global fetch/WS).

import { execFileSync } from "node:child_process";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-bm-ui");

const GENV = {
  GIT_AUTHOR_NAME: "bm",
  GIT_AUTHOR_EMAIL: "bm@test",
  GIT_COMMITTER_NAME: "bm",
  GIT_COMMITTER_EMAIL: "bm@test",
};
const git = (dir, args) =>
  execFileSync("git", args, { cwd: dir, env: { ...process.env, ...GENV } }).toString().trim();
function commit(dir, file, msg) {
  execFileSync("sh", ["-c", `printf '%s\\n' "${msg}" > ${file}`], { cwd: dir });
  git(dir, ["add", "."]);
  git(dir, ["commit", "-q", "-m", msg]);
}

async function main() {
  await sb.startAppDaemon();

  // main (A) + topic (A+B, one unique commit).
  const repo = sb.cwd;
  git(repo, ["init", "-q", "-b", "main"]);
  commit(repo, "a.txt", "A");
  git(repo, ["branch", "topic"]);
  git(repo, ["checkout", "-q", "topic"]);
  commit(repo, "b.txt", "B");
  git(repo, ["checkout", "-q", "main"]);

  const { workspace } = await sb.api("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "bm-repo", root_path: repo }),
  });
  check("git workspace registered", workspace.is_git === true);

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { evalJs, waitFor } = page;

  // The workspace row carries the (i) button in its trailing controls once the
  // workspace poll lands (only for Git workspaces).
  check(
    "workspace (i) button rendered",
    await waitFor("!!document.querySelector('.tree-actions .info-btn')"),
  );

  // Clicking it opens the branch-management dialog.
  await evalJs("document.querySelector('.tree-actions .info-btn').click()");
  check("branch dialog opened", await waitFor("!!document.querySelector('.branch-modal')"));

  check(
    "a branch row rendered per branch",
    await waitFor("document.querySelectorAll('.branch-modal .branch-row').length >= 2"),
    await evalJs("document.querySelectorAll('.branch-modal .branch-row').length"),
  );
  check(
    "both branch names shown",
    await waitFor(
      "/main/.test(document.querySelector('.branch-modal').innerText) && /topic/.test(document.querySelector('.branch-modal').innerText)",
    ),
  );
  check(
    "topic shows a merged-nowhere tag",
    // The tag is CSS-uppercased, and innerText reflects the transform — match
    // case-insensitively.
    await waitFor(
      "[...document.querySelectorAll('.branch-modal .branch-row')].some(r => /topic/.test(r.innerText) && /merged nowhere/i.test(r.innerText))",
    ),
  );
  check(
    "base 'ahead of' line rendered",
    await waitFor("/ahead of/.test(document.querySelector('.branch-modal').innerText)"),
  );
  check(
    "management controls present",
    await evalJs(
      "['Merge into','Rebase onto','Delete'].every(l => document.querySelector('.branch-modal').innerText.includes(l))",
    ),
  );

  // Close via the header Close button.
  await evalJs(
    "[...document.querySelectorAll('.branch-modal .modal-title button')].find(b=>/close/i.test(b.innerText))?.click()",
  );
  await sleep(300);
  check(
    "dialog closes",
    (await evalJs("!!document.querySelector('.branch-modal')")) === false,
  );

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the workspace (i) opens branch management and it renders branch rows");
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
