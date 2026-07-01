import { readFileSync, existsSync } from "node:fs";

const base = process.argv[2];
const repo = process.argv[3];
const http = `http://${base}`;
let fails = 0;
const check = (n, c, extra) => {
  console.log(`${c ? "PASS" : "FAIL"}  ${n}${extra ? "  " + extra : ""}`);
  if (!c) fails++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function j(path, init) {
  const res = await fetch(http + path, {
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

  console.log(fails === 0 ? "\nALL PASS" : `\n${fails} FAILURE(S)`);
  process.exit(fails === 0 ? 0 : 1);
}
main().catch((e) => {
  console.error(e);
  process.exit(2);
});
