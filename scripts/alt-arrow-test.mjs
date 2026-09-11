// Regression: xterm must send Alt+Up/Down, not its non-Mac Ctrl remapping.
// Exercises real browser keys, phone/tablet buttons, and delivery to a sandbox PTY.
// Run after `npm run build --prefix client` and `cargo build -p asm-daemon`:
//   node scripts/alt-arrow-test.mjs
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-alt-arrow");
const frame = (d) => JSON.stringify({ t: "i", d });

async function main() {
  await sb.startAppDaemon();
  await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  const chrome = await sb.startChrome();
  const { S, evalJs, waitFor } = await chrome.openPage("about:blank");
  await S("Page.addScriptToEvaluateOnNewDocument", { source: `
    window.__sent = [];
    const send = WebSocket.prototype.send;
    WebSocket.prototype.send = function(data) {
      if (typeof data === 'string') window.__sent.push(data);
      return send.call(this, data);
    };
  ` });
  const clear = () => evalJs("window.__sent.length = 0");
  const input = () => evalJs("window.__sent.filter(s => { try { return JSON.parse(s).t === 'i'; } catch { return false; } })");
  const focus = () => evalJs("document.querySelector('.xterm-helper-textarea').focus()");
  async function openSession() {
    await S("Page.navigate", { url: `${sb.http}/` });
    check("session listed", await waitFor("!!document.querySelector('.session-row')"));
    await evalJs("document.querySelector('.session-row').click()");
    check("terminal ready", await waitFor("!!document.querySelector('.xterm-helper-textarea') && !document.querySelector('.terminal-loading')"));
  }
  async function key(key, code, modifiers) {
    const args = { key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code, modifiers };
    await S("Input.dispatchKeyEvent", { ...args, type: "keyDown" });
    await S("Input.dispatchKeyEvent", { ...args, type: "keyUp" });
    await sleep(150);
  }
  await openSession();
  await focus();
  await S("Input.insertText", { text: "cat -v" });
  await sleep(300);
  await key("Enter", 13, 0);

  for (const [name, code, suffix] of [["ArrowUp", 38, "A"], ["ArrowDown", 40, "B"]]) {
    await clear();
    await key(name, code, 1); // CDP Alt modifier
    check(`${name}: Alt sends exactly one correct frame`, JSON.stringify(await input()) === JSON.stringify([frame(`\x1b[1;3${suffix}`)]));
    check(`${name}: Alt bytes reached PTY`, await waitFor(`document.querySelector('.terminal-mount').innerText.includes('^[[1;3${suffix}')`));
    await clear();
    await key(name, code, 2); // Ctrl stays Ctrl
    check(`${name}: Ctrl preserved`, JSON.stringify(await input()) === JSON.stringify([frame(`\x1b[1;5${suffix}`)]));
    await clear();
    await key(name, code, 0);
    check(`${name}: unmodified arrow preserved`, JSON.stringify(await input()) === JSON.stringify([frame(`\x1b[${suffix}`)]));
  }

  for (const [device, width, height, toolbar] of [
    ["phone", 390, 844, ".term-keybar"],
    ["tablet", 1024, 1366, ".term-panel"],
  ]) {
    await S("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 2, mobile: true });
    await S("Emulation.setTouchEmulationEnabled", { enabled: true });
    await openSession();
    if (device === "tablet") {
      check("tablet panel toggle available", await waitFor("!!document.querySelector('.term-panel-toggle')"));
      await evalJs("document.querySelector('.term-panel-toggle').click()");
    }
    check(`${device}: controls available`, await waitFor(`!!document.querySelector('${toolbar}')`));
    if (device === "phone") {
      check("phone: Alt+↑ is visible without scrolling", await evalJs(`(() => {
        const bar = document.querySelector('${toolbar}');
        const button = [...bar.querySelectorAll('button')].find(
          (item) => item.textContent.trim() === 'Alt+↑',
        );
        if (!button) return false;
        const barRect = bar.getBoundingClientRect();
        const buttonRect = button.getBoundingClientRect();
        return buttonRect.left >= barRect.left && buttonRect.right <= barRect.right;
      })()`));
    }
    for (const [label, suffix] of [["Alt+↑", "A"], ["Alt+↓", "B"]]) {
      await clear();
      check(`${device}: ${label} button available`, await evalJs(`(() => {
        const b = [...document.querySelectorAll('${toolbar} button')].find(b => b.textContent.trim() === ${JSON.stringify(label)});
        if (!b) return false;
        b.click(); return !!b.getAttribute('aria-label');
      })()`));
      check(`${device}: ${label} sends exactly one correct frame`, JSON.stringify(await input()) === JSON.stringify([frame(`\x1b[1;3${suffix}`)]));
    }
  }
  return report("Alt arrows preserve modifiers; phone and tablet expose both chords");
}
let ok = false;
try { ok = await main(); }
catch (e) { console.error(e); }
finally { sb.cleanup(); }
process.exit(ok ? 0 : 1);
