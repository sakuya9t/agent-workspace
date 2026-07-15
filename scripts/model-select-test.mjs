// End-to-end verification of the "select model" affordance on new-session/fork.
//
//   cd client && npm run build          # once, to produce client/dist
//   node scripts/model-select-test.mjs
//
// Two halves, both against a sandboxed throwaway daemon:
//
//   Backend  — GET /api/plugins/:id/models reports the right shape per agent
//              (coding agents support a dropdown, shell/custom_command don't),
//              detects each agent's currently-configured default from its own
//              config, and a chosen model actually reaches the launched argv
//              (`--model <m>` / `-m <m>`), while a shell ignores it.
//
//   UI       — selecting a coding agent reveals a Model dropdown whose default
//              entry is preselected and names the detected model, offers the
//              agent's models plus a "Custom…" free-text escape hatch.
//
// The agent-specific checks are gated on the agent being installed on this host
// (like fork-session-test), so the file is a clean pass on a box without them.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-model");

const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const models = (id) => sb.api(`/api/plugins/${id}/models`).then((r) => r.models);

async function main() {
  await sb.startAppDaemon();

  const plugins = (await sb.api("/api/plugins")).plugins;
  const avail = (id) => plugins.some((p) => p.id === id && p.available);

  // ---------- Backend: endpoint shape ----------

  const claude = await models("claude");
  check("claude supports a model dropdown", claude.supported === true);
  check(
    "claude offers its aliases (opus + sonnet) as choices",
    claude.models.some((m) => m.id === "opus") && claude.models.some((m) => m.id === "sonnet"),
    JSON.stringify(claude.models.map((m) => m.id)),
  );

  const codex = await models("codex");
  check("codex supports a model dropdown", codex.supported === true);
  check(
    "codex can't enumerate models, so its list is empty (Default + Custom only)",
    Array.isArray(codex.models) && codex.models.length === 0,
  );

  const opencode = await models("opencode");
  check("opencode supports a model dropdown", opencode.supported === true);
  check("opencode returns a model list array", Array.isArray(opencode.models));
  if (avail("opencode")) {
    // `opencode models` lists provider/model pairs; the base opencode/* set comes
    // from models.dev regardless of provider auth, so a fresh box still lists some.
    check(
      "opencode enumerates provider/model pairs",
      opencode.models.every((m) => m.id.includes("/")),
      JSON.stringify(opencode.models.slice(0, 3).map((m) => m.id)),
    );
  }

  const shell = await models("shell");
  check(
    "a shell has no model selector",
    shell.supported === false && shell.models.length === 0 && shell.default === null,
  );
  const custom = await models("custom_command");
  check("custom_command has no model selector", custom.supported === false);

  // ---------- Backend: default detection (from each agent's own config) ----------

  if (avail("claude")) {
    check(
      "claude's configured default model is detected",
      typeof claude.default === "string" && claude.default.length > 0,
      claude.default,
    );
  }
  if (avail("codex")) {
    check(
      "codex's configured default model is detected",
      typeof codex.default === "string" && codex.default.length > 0,
      codex.default,
    );
  }

  // ---------- Backend: the chosen model reaches the launched command ----------

  const repo = join(sb.tmp, "repo");
  execFileSync("mkdir", ["-p", repo]);

  if (avail("claude")) {
    const { session } = await sb.api("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "claude", cwd: repo, model: "sonnet" }),
    });
    check(
      "an overridden model reaches claude's argv as --model",
      eq(session.args, ["--model", "sonnet"]),
      JSON.stringify(session.args),
    );
    await sb.api(`/api/sessions/${session.id}/stop`, { method: "POST" });

    const { session: dflt } = await sb.api("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ agent_plugin_id: "claude", cwd: repo }),
    });
    check(
      "no override launches with no --model flag (agent's own default)",
      !dflt.args.includes("--model"),
      JSON.stringify(dflt.args),
    );
    await sb.api(`/api/sessions/${dflt.id}/stop`, { method: "POST" });
  }

  // A shell has no model selector, so a model sent for one is harmlessly dropped.
  const { session: sh } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: repo, model: "sonnet" }),
  });
  check(
    "a shell ignores a model it was handed (no --model in argv)",
    !sh.args.includes("--model") && !sh.args.includes("sonnet"),
    JSON.stringify(sh.args),
  );
  await sb.api(`/api/sessions/${sh.id}/stop`, { method: "POST" });

  // ---------- UI: the dropdown ----------

  if (!avail("claude")) {
    console.log("(claude not installed — skipping the UI half)");
    return;
  }

  const chrome = await sb.startChrome();
  const page = await chrome.openPage(`${sb.http}/`);
  const { evalJs, waitFor } = page;

  const NEW_BTN = "[...document.querySelectorAll('button.btn.primary')].find(x => (x.textContent || '').includes('New'))";
  check("the app rendered its New-session button", await waitFor(`!!(${NEW_BTN})`));

  // Helper injected into the page: set a dialog <select> (found by its label) the
  // way React expects (native setter + change event), and read a select's state.
  const SEL_HELPERS = `
    const selFor = (labelText) => {
      const l = [...document.querySelectorAll('.modal .form-label')]
        .find(x => x.textContent === labelText);
      if (!l) return null;
      let el = l.nextElementSibling;
      while (el && el.tagName !== 'SELECT') el = el.nextElementSibling;
      return el;
    };
    const setSel = (labelText, val) => {
      const el = selFor(labelText);
      const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
      set.call(el, val);
      el.dispatchEvent(new Event('change', { bubbles: true }));
    };
  `;

  await evalJs(`(${NEW_BTN}).click()`);
  check("new-session dialog opened", await waitFor("!!document.querySelector('.modal')"));

  await evalJs(`(() => { ${SEL_HELPERS} setSel('Agent', 'claude'); })()`);

  check(
    "selecting a coding agent reveals a Model dropdown",
    await waitFor(`(() => { ${SEL_HELPERS} return !!selFor('Model'); })()`),
  );

  const info = await evalJs(`(() => {
    ${SEL_HELPERS}
    const sel = selFor('Model');
    return {
      value: sel.value,
      opts: [...sel.options].map(o => ({ value: o.value, label: o.textContent })),
    };
  })()`);

  check("the default entry is preselected (launches with no flag)", info.value === "");
  check(
    "the default entry names the detected current model",
    info.opts[0].label.startsWith("Default") &&
      (!claude.default || info.opts[0].label.includes(claude.default)),
    info.opts[0].label,
  );
  check(
    "the dropdown lists the agent's models",
    info.opts.some((o) => o.value === "sonnet") && info.opts.some((o) => o.value === "opus"),
  );
  check(
    "a Custom… escape hatch is offered",
    info.opts.some((o) => o.label.includes("Custom")),
  );

  // Picking a listed model selects it.
  await evalJs(`(() => { ${SEL_HELPERS} setSel('Model', 'sonnet'); })()`);
  check(
    "picking a model updates the selection",
    (await evalJs(`(() => { ${SEL_HELPERS} return selFor('Model').value; })()`)) === "sonnet",
  );

  // Choosing Custom… reveals a free-text field.
  await evalJs(`(() => { ${SEL_HELPERS} setSel('Model', '__custom__'); })()`);
  check(
    "choosing Custom… reveals a free-text model field",
    await waitFor(
      "[...document.querySelectorAll('.modal input.input')].some(i => /model id/i.test(i.placeholder))",
    ),
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
