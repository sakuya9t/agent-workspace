// Repro of the desktop-companion report, against a sandboxed daemon:
//   1. register a plain folder → init-git → the default branch must exist
//   2. start a session in that workspace → worktree must be created (used to
//      die with `fatal: invalid reference: HEAD`)
//   3. a repo that was ALREADY `git init`-ed with no commits (the state
//      desktop-companion is in right now) → starting a session must repair it
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { createSandbox, checker } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-unborn");
const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

async function main() {
  await sb.startDaemon();

  // --- round 1: the new-workspace flow (folder → init git → session) ---
  const dir1 = join(sb.tmp, "fresh-folder");
  fs.mkdirSync(dir1);
  fs.writeFileSync(join(dir1, "notes.md"), "keep me untracked\n");

  const { workspace: ws1 } = await sb.api("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "fresh", root_path: dir1 }),
  });
  check("plain folder registered (not git)", ws1.is_git === false);

  const { workspace: ws1b } = await sb.api(`/api/workspaces/${ws1.id}/init-git`, {
    method: "POST",
    body: "{}",
  });
  check("init-git flips is_git", ws1b.is_git === true);

  const branch1 = git(dir1, "rev-parse", "--abbrev-ref", "HEAD");
  check("default branch exists after init", ["main", "master"].includes(branch1), branch1);
  check("HEAD resolves to a commit", !!git(dir1, "rev-parse", "--verify", "HEAD"));
  check(
    "user files left untracked by the bootstrap commit",
    git(dir1, "status", "--porcelain", "--untracked-files=all").includes("?? notes.md"),
  );

  const { session: s1 } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", workspace_id: ws1.id, cwd: "" }),
  });
  check("session starts in the init-git'd workspace", s1.status === "running", s1.id.slice(0, 8));

  const { instance: i1 } = await sb.api(`/api/sessions/${s1.id}/workspace`);
  check("session got an isolated worktree", i1?.isolation === "worktree", i1?.path);
  check("worktree is on an asm-session branch", i1?.branch?.startsWith("asm-session/"), i1?.branch);
  check("worktree directory exists", fs.existsSync(i1.path));

  // --- round 2: desktop-companion's current state — unborn repo from before the fix ---
  const dir2 = join(sb.tmp, "desktop-companion");
  fs.mkdirSync(dir2);
  git(dir2, "init"); // no commit: unborn HEAD, exactly what the old init-git left behind

  const { workspace: ws2 } = await sb.api("/api/workspaces", {
    method: "POST",
    body: JSON.stringify({ name: "desktop-companion", root_path: dir2 }),
  });
  check("existing unborn repo registers as git", ws2.is_git === true);

  const { session: s2 } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", workspace_id: ws2.id, cwd: "" }),
  });
  check("session starts against the unborn repo (was: invalid reference HEAD)", s2.status === "running");

  const { instance: i2 } = await sb.api(`/api/sessions/${s2.id}/workspace`);
  check("repair produced a real worktree", i2?.isolation === "worktree" && fs.existsSync(i2.path));
  const branch2 = git(dir2, "rev-parse", "--abbrev-ref", "HEAD");
  check("repair birthed the default branch in the source repo", ["main", "master"].includes(branch2), branch2);

  for (const s of [s1, s2]) {
    await sb.api(`/api/sessions/${s.id}/stop`, { method: "POST", body: "{}" }).catch(() => {});
  }
  return report("init-git births the default branch and unborn repos are repaired at session start");
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
