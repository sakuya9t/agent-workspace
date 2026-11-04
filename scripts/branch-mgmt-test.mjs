// Workspace-level git branch management: the /branches/overview endpoint reports
// per-branch session-attach counts, the base commit (same as the right panel),
// and how many commits are merged nowhere else; delete/merge/rebase act on
// branches with the right safety guards.
//
//   node scripts/branch-mgmt-test.mjs                 # sandboxed (default)
//
// This test CREATES branches, worktrees and sessions and DELETES/MERGES/REBASES
// branches, so it always builds a throwaway repo and drives a throwaway daemon.

import { execFileSync } from "node:child_process";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();

let sb = null;
let base;
let repo;

const GENV = {
  GIT_AUTHOR_NAME: "bm",
  GIT_AUTHOR_EMAIL: "bm@test",
  GIT_COMMITTER_NAME: "bm",
  GIT_COMMITTER_EMAIL: "bm@test",
};

function git(dir, args) {
  return execFileSync("git", args, { cwd: dir, env: { ...process.env, ...GENV } })
    .toString()
    .trim();
}

function commit(dir, file, msg) {
  execFileSync("sh", ["-c", `printf '%s\\n' "${msg}" > ${file}`], { cwd: dir });
  git(dir, ["add", "."]);
  git(dir, ["commit", "-q", "-m", msg]);
}

/**
 * A repo where, off `main` (commit A):
 *  - topic = A + B         (one unique commit)
 *  - lonely = A + L        (one unique commit, not checked out)
 *  - rb = A + R            (to rebase onto an advanced main later)
 * main itself has no unique commit (A is contained everywhere).
 */
function makeRepo(dir) {
  git(dir, ["init", "-q", "-b", "main"]);
  commit(dir, "a.txt", "A");
  git(dir, ["branch", "topic"]);
  git(dir, ["checkout", "-q", "topic"]);
  commit(dir, "b.txt", "B");
  git(dir, ["checkout", "-q", "main"]);
  git(dir, ["branch", "lonely"]);
  git(dir, ["checkout", "-q", "lonely"]);
  commit(dir, "l.txt", "L");
  git(dir, ["checkout", "-q", "main"]);
  git(dir, ["branch", "rb"]);
  git(dir, ["checkout", "-q", "rb"]);
  commit(dir, "r.txt", "R");
  git(dir, ["checkout", "-q", "main"]);
  return dir;
}

const http = () => `http://${base}`;

async function j(path, init) {
  const res = await fetch(http() + path, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  const text = await res.text();
  if (!res.ok) {
    const err = new Error(`${path} -> ${res.status} ${text}`);
    err.status = res.status;
    throw err;
  }
  return text ? JSON.parse(text) : null;
}

async function main() {
  sb = await createSandbox("asm-bm");
  await sb.startDaemon();
  base = sb.base;
  repo = makeRepo(sb.cwd);
  console.log(`sandbox daemon on ${base}  (repo: ${repo})\n`);

  const { workspace } = await j("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "bm-repo", root_path: repo }),
  });
  check("workspace registered as git", workspace.is_git === true);
  const wsId = workspace.id;

  // s1 on the EXISTING `topic` branch → a worktree that checks it out.
  const s1 = (
    await j("/api/sessions", {
      method: "POST",
      body: JSON.stringify({
        agent_plugin_id: "shell",
        workspace_id: wsId,
        branch: "topic",
        create_branch: false,
      }),
    })
  ).session;
  // s2 with no branch → an auto `asm-session/<id8>` branch this daemon owns.
  const s2 = (
    await j("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "shell", workspace_id: wsId }),
    })
  ).session;
  const autoBranch = `asm-session/${s2.id.slice(0, 8)}`;
  await sleep(300);

  const overview1 = (await j(`/api/workspaces/${wsId}/branches/overview`)).overview;
  check("overview reports a git repo", overview1.is_git === true);
  const by = Object.fromEntries(overview1.branches.map((b) => [b.name, b]));

  // (1) sessions attached to each branch
  check(
    "topic has session s1 attached",
    by.topic && by.topic.session_ids.includes(s1.id),
    JSON.stringify(by.topic?.session_ids),
  );
  check(
    "auto branch has session s2 attached",
    by[autoBranch] && by[autoBranch].session_ids.includes(s2.id),
    autoBranch,
  );
  check("auto branch is marked owned by asm", by[autoBranch]?.owns_branch === true);
  check("lonely has no sessions attached", by.lonely && by.lonely.session_ids.length === 0);

  // (2) ahead of its base (same base the right panel shows)
  check("topic is 1 commit ahead of its base", by.topic?.base?.ahead === 1, JSON.stringify(by.topic?.base));
  check("lonely is 1 commit ahead of its base", by.lonely?.base?.ahead === 1);

  // (3) commits merged nowhere
  check("topic has 1 commit merged nowhere", by.topic?.unmerged_commits === 1);
  check("lonely has 1 commit merged nowhere", by.lonely?.unmerged_commits === 1);
  check("main has nothing unique (merged everywhere)", by.main?.unmerged_commits === 0);

  // topic is checked out in s1's worktree → deletion blocked.
  check("topic reports a checked-out worktree", !!by.topic?.checked_out_path);

  // ---- management ----

  // Deleting a checked-out branch is refused.
  let topicDeleteErr = "";
  try {
    await j(`/api/workspaces/${wsId}/branches/delete`, {
      method: "POST",
      body: JSON.stringify({ branch: "topic" }),
    });
  } catch (e) {
    topicDeleteErr = String(e);
  }
  check("delete of a checked-out branch is refused", /checked out/.test(topicDeleteErr));

  // Deleting an unmerged branch needs force (409 first).
  let lonely409 = 0;
  try {
    await j(`/api/workspaces/${wsId}/branches/delete`, {
      method: "POST",
      body: JSON.stringify({ branch: "lonely" }),
    });
  } catch (e) {
    lonely409 = e.status;
  }
  check("delete of an unmerged branch returns 409", lonely409 === 409);

  await j(`/api/workspaces/${wsId}/branches/delete`, {
    method: "POST",
    body: JSON.stringify({ branch: "lonely", force: true }),
  });
  const afterDel = (await j(`/api/workspaces/${wsId}/branches/overview`)).overview;
  check("force-delete removed lonely", !afterDel.branches.some((b) => b.name === "lonely"));

  // Merge topic into main (source checked out live is fine; target isn't live).
  const merge = await j(`/api/workspaces/${wsId}/branches/merge`, {
    method: "POST",
    body: JSON.stringify({ source: "topic", target: "main" }),
  });
  check("merge reports success", /Merged topic into main/.test(merge.output), merge.output);
  const afterMerge = (await j(`/api/workspaces/${wsId}/branches/overview`)).overview;
  const topicAfter = afterMerge.branches.find((b) => b.name === "topic");
  check("topic is merged nowhere-count now 0 after merge", topicAfter?.unmerged_commits === 0);

  // Rebasing a branch a LIVE session sits on is refused.
  let liveRebaseErr = "";
  try {
    await j(`/api/workspaces/${wsId}/branches/rebase`, {
      method: "POST",
      body: JSON.stringify({ branch: autoBranch, onto: "main" }),
    });
  } catch (e) {
    liveRebaseErr = String(e);
  }
  check("rebase of a live session's branch is refused", /running session/.test(liveRebaseErr));

  // Rebase a bare branch (rb) onto the now-advanced main.
  const rebase = await j(`/api/workspaces/${wsId}/branches/rebase`, {
    method: "POST",
    body: JSON.stringify({ branch: "rb", onto: "main" }),
  });
  check("rebase reports success", /Rebased rb onto main/.test(rebase.output), rebase.output);
  // rb now carries main's B beneath its own R.
  check("rb now contains main's merged commit", git(repo, ["show", "rb:b.txt"]).trim() === "B");
  check("rb kept its own work", git(repo, ["show", "rb:r.txt"]).trim() === "R");
  // No temp worktree left behind: main + s1 + s2 worktrees only (3).
  const wtCount = git(repo, ["worktree", "list", "--porcelain"]).match(/worktree /g)?.length ?? 0;
  check("no leftover temp worktree after rebase", wtCount === 3, `worktrees=${wtCount}`);

  return report("workspace branch overview + delete/merge/rebase behave and stay guarded");
}

main()
  .then((pass) => {
    sb?.cleanup();
    process.exit(pass ? 0 : 1);
  })
  .catch((e) => {
    console.error(e);
    sb?.cleanup();
    process.exit(2);
  });
