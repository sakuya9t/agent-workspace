// Real Git + browser regressions. A sandbox-only codex input sink avoids models.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { join } from "node:path";
import { createSandbox, sleep } from "./lib/testenv.mjs";

const sb = await createSandbox("asm-conflict-prompt");
const git = (...args) => execFileSync("git", args, { cwd: sb.cwd, encoding: "utf8" }).trim();

async function main() {
  const bin = join(sb.tmp, "bin");
  fs.mkdirSync(bin);
  fs.writeFileSync(join(bin, "codex"), "#!/bin/sh\nexec /bin/cat\n", { mode: 0o755 });
  await sb.startAppDaemon("daemon", { PATH: bin + ":" + process.env.PATH });
  git("init", "-q", "-b", "main");
  git("config", "user.email", "test@example.com");
  git("config", "user.name", "test");
  const commit = (text) => {
    fs.writeFileSync(join(sb.cwd, "conflict.txt"), text + "\n");
    git("add", "conflict.txt");
    git("commit", "-qm", text);
  };
  commit("base");
  git("checkout", "-qb", "feature");
  commit("feature");
  git("branch", "clean-target");
  git("checkout", "-q", "main");
  commit("main");
  git("checkout", "-q", "feature");
  const create = async (agent_plugin_id, extra = {}) => (await sb.api("/api/sessions", {
    method: "POST", body: JSON.stringify({ agent_plugin_id, cwd: sb.cwd, ...extra }),
  })).session;
  const agent = await create("codex");
  const other = await create("codex");
  const shell = await create("shell");
  const custom = await create("custom_command", { command: "/bin/cat", approve_custom: true });

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(sb.http + "/");
  const { evalJs, waitFor } = page;
  // Only labels are replaced to select exact sessions independent of sorting.
  // Session capabilities and Git responses are real; input frames are intercepted.
  await evalJs("(" + (() => {
    window.__frames = [];
    window.__sockets = [];
    window.__responses = [];
    window.confirm = () => true;
    const OriginalSocket = window.WebSocket;
    window.WebSocket = class extends OriginalSocket {
      constructor(...args) { super(...args); window.__sockets.push(this); }
    };
    const send = OriginalSocket.prototype.send;
    OriginalSocket.prototype.send = function (data) {
      const message = JSON.parse(data);
      if (message.t === "i") {
        window.__frames.push({ data: message.d, url: this.url });
        if (window.__dropAfterPrompt) {
          window.__dropAfterPrompt = false;
          this.close();
        }
        return;
      }
      return send.call(this, data);
    };
    const originalFetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await originalFetch(...args);
      const url = String(args[0]);
      if (url.endsWith("/api/sessions") && !args[1]?.method) {
        const body = await response.json();
        body.sessions.forEach(s => { s.title = s.id; });
        return new Response(JSON.stringify(body), { status: response.status });
      }
      if (/\/scm\/(merge|rebase)$/.test(url)) {
        window.__responses.push({ url, status: response.status });
        if (window.__holdOperation) {
          window.__holdOperation = false;
          await new Promise(resolve => { window.__release = resolve; });
        }
      }
      return response;
    };
  }).toString() + ")()");
  const wait = async (expr) => assert.ok(await waitFor(expr, 15000), expr);
  const row = (id) => "[...document.querySelectorAll('.session-row')].find(r => r.querySelector('.session-title')?.textContent === " + JSON.stringify(id) + ")";
  const select = async (s) => {
    await wait("!!(" + row(s.id) + ")");
    await evalJs("(" + row(s.id) + ").click()");
    await wait("(" + row(s.id) + ")?.classList.contains('active')");
    await wait("window.__sockets.some(s => s.url.includes(" + JSON.stringify(s.id) + ") && s.readyState === 1)");
    await wait("!!document.querySelector('.action-icon-git-merge')");
  };
  const run = async (op, target = "main") => {
    await evalJs("window.__frames = []; window.__responses = [];");
    const button = "document.querySelector('.action-icon-git-" + op + "').closest('button')";
    if (await evalJs(button + ".classList.contains('active')")) await evalJs(button + ".click()");
    await evalJs(button + ".click()");
    await wait("!!document.querySelector('.rebase-picker select option[value=\"" + target + "\"]')");
    await evalJs("(() => { const select = document.querySelector('.rebase-picker select'); " +
      "Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, " + JSON.stringify(target) + "); " +
      "select.dispatchEvent(new Event('change', { bubbles: true })); })()");
    await wait("!document.querySelector('.rebase-picker .btn').disabled");
    await evalJs("document.querySelector('.rebase-picker .btn').click()");
    await wait("window.__responses.length === 1");
    assert.equal(await evalJs("window.__responses[0].status"), target === "main" ? 409 : 200);
  };
  const notice = (text) => "document.querySelector('.details')?.textContent.includes(" + JSON.stringify(text) + ")";
  const manual = async () => {
    await wait(notice("The request was not submitted to an agent."));
    assert.ok(!await evalJs(notice("was handed to the current session agent")));
  };
  const silent = async () => assert.deepEqual(await evalJs("window.__frames"), []);

  await select(agent);
  for (const op of ["merge", "rebase"]) {
    await run(op, "clean-target");
    await wait(notice(op === "merge" ? "Merge complete" : "Rebase complete"));
    await silent();
    await run(op);
    await wait(notice("was handed to the current session agent"));
    const frames = await evalJs("window.__frames");
    assert.equal(frames.length, 2);
    assert.ok(frames.every(f => f.url.includes(agent.id)));
    assert.ok(frames[0].data.startsWith(op === "merge"
      ? "merge current branch to target branch main, resolving conflicts."
      : "rebase current branch onto target branch main, resolving conflicts."));
    assert.match(frames[0].data, /leave it for a human/);
    assert.equal(frames[1].data, "\r");
    console.log("PASS " + op + ": clean is silent; conflict submits to the owning agent");
  }

  for (const op of ["merge", "rebase"]) {
    await select(agent);
    await evalJs("window.__holdOperation = true; window.__release = null;");
    await run(op);
    await wait("typeof window.__release === 'function'");
    await select(other);
    await evalJs("window.__release(); true");
    await sleep(600);
    await silent();
    assert.ok(!await evalJs(notice("was handed to the current session agent")));
    console.log("PASS " + op + ": switching sessions never redirects the prompt");
  }

  for (const s of [shell, custom]) {
    await select(s);
    for (const op of ["merge", "rebase"]) {
      await run(op);
      await manual();
      await silent();
    }
    console.log("PASS " + s.agent_plugin_id + ": conflicts remain manual");
  }

  await select(agent);
  await evalJs("window.__dropAfterPrompt = true;");
  await run("merge");
  await manual();
  assert.equal((await evalJs("window.__frames")).length, 1);
  console.log("PASS connection lost before Enter: notice reports no submission");

  await sb.api("/api/sessions/" + agent.id + "/stop?force=true", { method: "POST" });
  await wait(notice("stopped"));
  for (const op of ["merge", "rebase"]) {
    await run(op);
    await manual();
    await silent();
  }
  console.log("PASS stopped agent: conflicts remain manual");
  await chrome.send("Target.closeTarget", { targetId: page.targetId });
  chrome.ws.close();
}

try { await main(); } finally { await sb.cleanup(); }
