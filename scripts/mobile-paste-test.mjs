// Headless-Chrome verification of the mobile key bar's PASTE, in both of the
// contexts a phone can find itself in.
//
// The bug: `navigator.clipboard.readText()` exists only in a SECURE context
// (HTTPS or localhost), and the daemon and the relay both serve plain HTTP — so
// on a real phone the key bar's Paste had nothing to read and did nothing at all,
// in silence. It was invisible from a dev machine because localhost IS a secure
// context: the identical code passes in Chrome's device emulation and fails on
// the device. Copy meanwhile kept working, through its execCommand fallback,
// which is what made the pair so confusing to look at.
//
// The fix (PasteSheet.tsx) leans on the one clipboard path needing neither a
// secure context nor a permission: a `paste` EVENT carries its own clipboardData,
// because the OS hands the text over precisely when the user chooses to paste. An
// unreadable clipboard therefore opens a focused textarea to paste INTO, and what
// lands there goes to the pty.
//
// The insecure context is SIMULATED — `isSecureContext: false` and no
// `navigator.clipboard`, which is exactly what a browser hands a page on http://
// — because a sandbox on loopback cannot serve a genuinely insecure origin. (The
// desktop suite takes the other road: copy-paste-test.mjs's T3 wants a real LAN
// origin via `vite --host`, and is opt-in for that reason.)
//
//   cd client && npm run build           # once, to produce client/dist
//   node scripts/mobile-paste-test.mjs   # sandboxed: daemon + chrome + session
//
// Node 18+ (global fetch/WS). See also scripts/touch-select-test.mjs.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-mpaste");

const SECURE_TEXT = "PASTED_BY_CLIPBOARD_READ";
const SHEET_TEXT = "PASTED_BY_SHEET";

/** Put the one page on a phone-sized viewport with touch. */
async function asPhone(page) {
  const { S } = page;
  // 390×844 matches PHONE_MQ, so the mobile shell (and its key bar) is what mounts.
  await S("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 3,
    mobile: true,
  });
  await S("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  await S("Network.setCacheDisabled", { cacheDisabled: true }); // else a stale index.html runs the OLD bundle
  await S("Emulation.setFocusEmulationEnabled", { enabled: true }); // a clipboard read needs a focused document
}

/** Load the client and drill into the session's terminal screen. Both contexts run
 *  in ONE tab, navigated twice — a second CDP target wedges this harness. */
async function enterTerminal(page) {
  const { S, evalJs, waitFor } = page;
  await S("Page.navigate", { url: `${sb.http}/` });
  await waitFor("!!document.querySelector('.session-row')");
  await evalJs("document.querySelector('.session-row').click()");
  await waitFor("!!document.querySelector('.terminal-host')");
  await waitFor("(document.querySelector('.terminal-host')?.innerText||'').trim().length > 0");
}

/** Take the clipboard API away for every subsequent load: exactly what a browser
 *  hands a page served over plain http, which is what the phone gets. */
async function goInsecure(page) {
  await page.S("Page.addScriptToEvaluateOnNewDocument", {
    source: `
      Object.defineProperty(window, "isSecureContext", { value: false, configurable: true });
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    `,
  });
}

/** Tap a key-bar button by its label, the way a finger would — a REAL touch, not
 *  element.click(), because a clipboard read is only granted to a genuine user
 *  gesture. The bar scrolls horizontally and Paste/Copy sit off the right edge of
 *  a 390px screen, so scroll it into view first or the tap lands on nothing. */
async function tapKey(page, label) {
  const at = await page.evalJs(`(async () => {
    const b = [...document.querySelectorAll('.term-keybar .kb')]
      .find(x => x.textContent.trim() === ${JSON.stringify(label)});
    if (!b) return null;
    b.scrollIntoView({ block: 'nearest', inline: 'center' });
    await new Promise(r => setTimeout(r, 250)); // let the scroll settle
    const r = b.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!at) throw new Error(`no key-bar button labelled "${label}"`);
  if (at.x < 0 || at.x > 390) throw new Error(`"${label}" is off-screen at x=${at.x}`);
  await page.S("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [at] });
  await page.S("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await sleep(500);
}

/** The shell echoes whatever is typed at its prompt, so text that actually reached
 *  the pty comes back and paints in the terminal. Whitespace is stripped before the
 *  match: a phone terminal is ~44 columns, the prompt eats most of a row, and the
 *  echo wraps — putting a newline through the middle of the marker. */
const echoedByPty = (page, text) =>
  page.waitFor(
    `document.querySelector('.terminal-host').innerText.replace(/\\s+/g, '').includes(${JSON.stringify(text)})`,
    6000,
  );

const sheetIsOpen = (page) => page.evalJs("!!document.querySelector('.paste-sheet')");

async function main() {
  await sb.startAppDaemon();
  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  check("session created for the UI", session.status === "running", session.id.slice(0, 8));

  const chrome = await sb.startChrome();
  await chrome.send("Browser.grantPermissions", {
    origin: sb.http,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });
  const page = await chrome.openPage("about:blank");
  await asPhone(page);

  // ------------------------------------------------------------------ SECURE
  // Where the clipboard CAN be read, Paste stays one tap. This is the path that
  // always worked — and always passed on a dev machine, which is the whole problem.
  await enterTerminal(page);
  check(
    "secure context: the clipboard is readable",
    await page.evalJs("window.isSecureContext && !!navigator.clipboard?.readText"),
  );
  await page.evalJs(`navigator.clipboard.writeText(${JSON.stringify(SECURE_TEXT)})`);
  await tapKey(page, "Paste");
  check("secure: Paste writes the clipboard to the pty", await echoedByPty(page, SECURE_TEXT));
  check("secure: Paste does not open the fallback sheet", !(await sheetIsOpen(page)));

  // ---------------------------------------------------------------- INSECURE
  // The phone's actual conditions. Before the fix this tap did nothing whatsoever.
  const plain = page;
  await goInsecure(plain);
  await enterTerminal(plain); // reload: the stub lands on the fresh document
  check(
    "insecure context: there is no clipboard to read (the phone's case)",
    await plain.evalJs("!window.isSecureContext && !navigator.clipboard"),
  );
  await tapKey(plain, "Paste");
  check("insecure: Paste opens the fallback sheet rather than failing silently", await sheetIsOpen(plain));
  const sheetBounds = await plain.evalJs(`(() => {
    const rect = (selector) => {
      const r = document.querySelector(selector)?.getBoundingClientRect();
      return r && { left: r.left, right: r.right, width: r.width };
    };
    return { viewport: window.innerWidth, sheet: rect('.paste-sheet'), input: rect('.paste-sheet-input') };
  })()`);
  check(
    "insecure: paste sheet and input stay inside the phone viewport",
    [sheetBounds.sheet, sheetBounds.input].every(
      (r) => r && r.left >= -0.5 && r.right <= sheetBounds.viewport + 0.5,
    ),
    JSON.stringify(sheetBounds),
  );
  check(
    "insecure: the sheet's input is focused, so the keyboard rises with it",
    await plain.evalJs("document.activeElement === document.querySelector('.paste-sheet-input')"),
  );

  // The paste EVENT is the whole point: it carries clipboardData with no secure
  // context and no permission, because the user chose to paste. (Returns false
  // against an unfixed client, where there is no sheet — the checks below then
  // FAIL rather than the run blowing up.)
  await plain.evalJs(`(() => {
    const input = document.querySelector('.paste-sheet-input');
    if (!input) return false;
    const dt = new DataTransfer();
    dt.setData('text', ${JSON.stringify(SHEET_TEXT)});
    input.dispatchEvent(
      new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }),
    );
    return true;
  })()`);
  check(
    "insecure: pasting into the sheet reaches the pty",
    await echoedByPty(plain, SHEET_TEXT),
    "no clipboard read anywhere in it",
  );
  check(
    "insecure: the sheet closes once the text is through",
    await plain.waitFor("!document.querySelector('.paste-sheet')", 3000),
  );

  // A dismissed sheet must not leave half a paste behind.
  await tapKey(plain, "Paste");
  await plain.evalJs(
    "[...document.querySelectorAll('.paste-sheet .btn')].find(b => b.textContent.trim() === 'Cancel')?.click()",
  );
  await sleep(300);
  check("insecure: Cancel closes the sheet and writes nothing", !(await sheetIsOpen(plain)));

  return report("mobile paste");
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
