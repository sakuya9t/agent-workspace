// Per-session git worktree isolation: two sessions on one workspace get
// distinct worktrees, cannot collide on the same file, and honour the cleanup
// guards (live / dirty / force).
//
//   node scripts/worktree-test.mjs                      # sandboxed (default)
//   node scripts/worktree-test.mjs 127.0.0.1:4600 /repo # ATTENDED: real daemon + real repo
//
// This test CREATES and FORCE-REMOVES git worktrees, so by default it builds a
// throwaway repo and drives a throwaway daemon. Pointing it at a real daemon and
// a real repo means it will make and delete worktrees in *that* repo.

import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const attendedBase = process.argv[2] ?? null;
const { check, report } = checker();

let sb = null;
let base;
let repo;

/** A throwaway git repo with one commit — enough for `is_git` and worktrees. */
function makeRepo(dir) {
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: "wt",
    GIT_AUTHOR_EMAIL: "wt@test",
    GIT_COMMITTER_NAME: "wt",
    GIT_COMMITTER_EMAIL: "wt@test",
  };
  execFileSync("git", ["init", "-q", "-b", "main"], { cwd: dir, env });
  execFileSync("git", ["commit", "-q", "--allow-empty", "-m", "init"], { cwd: dir, env });
  return dir;
}

async function setup() {
  if (attendedBase) {
    base = attendedBase;
    repo = process.argv[3];
    if (!repo) throw new Error("attended mode needs a repo path: worktree-test.mjs <base> <repo>");
    console.log(`!! ATTENDED MODE — ${base}, worktrees will be created in ${repo}\n`);
    return;
  }
  sb = await createSandbox("asm-wt");
  await sb.startDaemon();
  base = sb.base;
  repo = makeRepo(sb.cwd);
  console.log(`sandbox daemon on ${base}  (repo: ${repo})\n`);
}

const http = () => `http://${base}`;

async function j(path, init) {
  const res = await fetch(http() + path, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${path} -> ${res.status} ${text}`);
  return JSON.parse(text);
}

async function sendCmd(id, cmd) {
  const ws = new WebSocket(`ws://${base}/api/sessions/${id}/stream`);
  ws.binaryType = "arraybuffer";
  await new Promise((r) => (ws.onopen = r));
  await sleep(400);
  ws.send(JSON.stringify({ t: "i", d: cmd + "\r" }));
  await sleep(600);
  ws.close();
  await sleep(150);
}

async function main() {
  await setup();

  const { workspace } = await j("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "test-repo", root_path: repo }),
  });
  check("workspace registered as git", workspace.is_git === true);

  const mk = () =>
    j("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "shell", workspace_id: workspace.id }),
    }).then((r) => r.session);

  const s1 = await mk();
  const s2 = await mk();

  const i1 = (await j(`/api/sessions/${s1.id}/workspace`)).instance;
  const i2 = (await j(`/api/sessions/${s2.id}/workspace`)).instance;

  check("session1 got a worktree", i1?.isolation === "worktree", i1?.branch);
  check("session2 got a worktree", i2?.isolation === "worktree", i2?.branch);
  check("worktrees are distinct paths", i1.path !== i2.path);
  check("worktree1 exists on disk", existsSync(i1.path));
  check("worktree2 exists on disk", existsSync(i2.path));

  // Edit the SAME relative file in each worktree via its own shell.
  await sendCmd(s1.id, "printf 'from-s1\\n' > shared.txt");
  await sendCmd(s2.id, "printf 'from-s2\\n' > shared.txt");

  const c1 = readFileSync(`${i1.path}/shared.txt`, "utf8").trim();
  const c2 = readFileSync(`${i2.path}/shared.txt`, "utf8").trim();
  check("no collision: worktree1 sees its own write", c1 === "from-s1", c1);
  check("no collision: worktree2 sees its own write", c2 === "from-s2", c2);

  // Cleanup guards: dirty worktree while live -> blocked.
  let blockedLive = false;
  try {
    await j(`/api/sessions/${s1.id}/cleanup`, { method: "POST" });
  } catch (e) {
    blockedLive = /stop the session/.test(String(e));
  }
  check("cleanup blocked while session live", blockedLive);

  await j(`/api/sessions/${s1.id}/stop`, { method: "POST" });
  await sleep(300);

  let blockedDirty = false;
  try {
    await j(`/api/sessions/${s1.id}/cleanup`, { method: "POST" });
  } catch (e) {
    blockedDirty = /uncommitted changes/.test(String(e));
  }
  check("cleanup blocked when dirty (no force)", blockedDirty);

  await j(`/api/sessions/${s1.id}/cleanup?force=true`, { method: "POST" });
  await sleep(200);
  check("forced cleanup removed worktree1", !existsSync(i1.path));
  check("worktree2 still intact", existsSync(i2.path));

  return report("per-session worktrees isolate and clean up safely");
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
