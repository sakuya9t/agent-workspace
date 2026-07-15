// Headless-Chrome verification of the terminal's touch clipboard buttons — the
// Copy/Paste pair that sits next to 📎 in the terminal's bottom-right action row.
//
// The gap they close: a tablet takes the DESKTOP shell (useIsPhone is a layout
// test — an iPad is desktop-shaped), so it never sees the phone key bar's
// Copy/Paste; and having no keyboard, it can't press ⌘-C / Ctrl-Shift-C either.
// So an iPad could *select* text (the long-press gesture works there already,
// see touch-select-test.mjs) and then had no way to copy it, nor any way to
// paste. The buttons are therefore gated on `(pointer: coarse) && !phone`.
//
//   cd client && npm run build             # once, to produce client/dist
//   node scripts/touch-clipboard-test.mjs  # sandboxed: daemon + chrome + session
//
// The three device classes are all asserted here, because the bug this guards
// against is a button appearing in the wrong one (a duplicate Copy on the phone,
// or a touch-only control on a mouse desktop):
//
//   iPad   1024×768 coarse → desktop shell, NO key bar, buttons PRESENT
//   phone   390×844 coarse → mobile shell, key bar (its own Copy/Paste), NO buttons
//   desktop 1280×800 fine  → desktop shell, NO buttons (⌘-C / Ctrl-Shift-C instead)
//
// Chrome gotcha, learned the hard way: `Emulation.setEmulatedMedia` does NOT
// support the `pointer` media feature — it silently no-ops and every assertion
// below would pass for the wrong reason. Only setDeviceMetricsOverride(mobile)
// + setTouchEmulationEnabled actually flips `(pointer: coarse)`, so the tests
// assert the media query itself before trusting anything downstream.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-clip");

/** A lone word on its own output row: the cleanest long-press target. */
const MARKER = "COPYME";

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
  const { S, evalJs, waitFor } = page;

  await S("Network.setCacheDisabled", { cacheDisabled: true }); // else a stale index.html runs the OLD bundle
  // navigator.clipboard refuses to read/write unless the document is focused,
  // and a headless page isn't until something real lands on it.
  await S("Emulation.setFocusEmulationEnabled", { enabled: true });

  /** Put the page in a device class. `coarse` is what gates the buttons. */
  const emulate = async ({ width, height, coarse }) => {
    await S("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 2,
      mobile: coarse,
    });
    // maxTouchPoints must stay in 1..16 even when disabling — CDP rejects 0.
    await S("Emulation.setTouchEmulationEnabled", { enabled: coarse, maxTouchPoints: 5 });
  };

  // The terminal is only ~40 columns wide in the desktop shell's centre panel, so
  // pasted text hard-wraps and innerText grows a newline mid-word. Match against
  // the de-wrapped text or the assertion fails on a line break rather than a bug.
  const termTextDewrapped = () =>
    evalJs("document.querySelector('.terminal-host').innerText.replace(/\\s+/g, '')");

  /** Reload into the session's terminal and wait for it to paint. */
  const openTerminal = async () => {
    await S("Page.navigate", { url: `${sb.http}/` });
    await waitFor("!!document.querySelector('.session-row')");
    await evalJs("document.querySelector('.session-row').click()");
    await waitFor("!!document.querySelector('.terminal-host')");
    await waitFor("(document.querySelector('.terminal-host')?.innerText||'').trim().length > 0");
    await sleep(300);
  };

  const has = (sel) => evalJs(`!!document.querySelector('${sel}')`);
  const statusText = () =>
    evalJs("(document.querySelector('.paste-status')?.textContent || '').trim()");
  const clipboard = () => evalJs("navigator.clipboard.readText().catch(() => '')");
  const setClipboard = (t) =>
    evalJs(`navigator.clipboard.writeText(${JSON.stringify(t)}).then(()=>true).catch(()=>false)`);
  const click = async (sel) => {
    await evalJs(`document.querySelector('${sel}').click()`);
    await sleep(400);
  };

  // ================================================================ iPad
  // Desktop-shaped, but driven by a finger: the whole reason these buttons exist.
  await emulate({ width: 1024, height: 768, coarse: true });
  await openTerminal();

  check("iPad: (pointer: coarse) matches", await evalJs("matchMedia('(pointer: coarse)').matches"));
  check(
    "iPad: NOT phone-class, so it takes the desktop shell",
    !(await evalJs(
      "matchMedia('(max-width: 599px), ((max-height: 599px) and (pointer: coarse))').matches",
    )),
  );
  check("iPad: no phone key bar (that's the gap being closed)", !(await has(".term-keybar")));
  check("iPad: Copy button rendered", await has(".term-actions .term-copy"));
  check("iPad: Paste button rendered", await has(".term-actions .term-paste"));
  check("iPad: 📎 attach button still rendered beside them", await has(".term-actions .term-attach"));
  check(
    "iPad: buttons carry accessible labels",
    (await evalJs("document.querySelector('.term-copy').getAttribute('aria-label')")) ===
      "Copy selection" &&
      (await evalJs("document.querySelector('.term-paste').getAttribute('aria-label')")) === "Paste",
  );

  // Geometry: three separate tap targets, all inside the terminal, none overlapping.
  const layout = await evalJs(`(() => {
    const host = document.querySelector('.terminal-host').getBoundingClientRect();
    const b = (s) => { const r = document.querySelector(s).getBoundingClientRect();
      return { x: r.x, y: r.y, w: r.width, h: r.height, right: r.right, bottom: r.bottom }; };
    const rects = { copy: b('.term-copy'), paste: b('.term-paste'), attach: b('.term-attach') };
    const overlap = (a, c) =>
      a.x < c.right && c.x < a.right && a.y < c.bottom && c.y < a.bottom;
    const inside = (r) =>
      r.x >= host.x - 1 && r.y >= host.y - 1 && r.right <= host.right + 1 && r.bottom <= host.bottom + 1;
    return {
      rects,
      sized: Object.values(rects).every((r) => r.w >= 30 && r.h >= 30),
      inside: Object.values(rects).every(inside),
      disjoint: !overlap(rects.copy, rects.paste) && !overlap(rects.paste, rects.attach) &&
                !overlap(rects.copy, rects.attach),
      ordered: rects.copy.x < rects.paste.x && rects.paste.x < rects.attach.x,
    };
  })()`);
  check("iPad: all three buttons are ≥30px tap targets", layout.sized, JSON.stringify(layout.rects));
  check("iPad: all three sit inside the terminal host", layout.inside);
  check("iPad: none of the three overlap", layout.disjoint);
  check("iPad: laid out as [copy] [paste] 📎", layout.ordered);

  // ---- Copy with nothing selected must SAY so, not no-op silently.
  await setClipboard("__NOTHING_COPIED__");
  await click(".term-copy");
  const emptyMsg = await statusText();
  check("iPad: Copy with no selection explains itself", emptyMsg === "Select some text first", emptyMsg);
  check(
    "iPad: …and does not touch the clipboard",
    (await clipboard()) === "__NOTHING_COPIED__",
  );

  // ---- Copy a real selection, made the way a finger makes one (long-press).
  await evalJs("document.querySelector('.xterm-helper-textarea').focus()");
  await S("Input.insertText", { text: `echo ${MARKER}` });
  await S("Input.dispatchKeyEvent", { type: "keyDown", windowsVirtualKeyCode: 13, key: "Enter" });
  await S("Input.dispatchKeyEvent", { type: "keyUp", windowsVirtualKeyCode: 13, key: "Enter" });
  const painted = await waitFor(
    `(() => { const t = document.querySelector('.terminal-host').innerText;
       return (t.match(/^${MARKER}$/gm) || []).length >= 1; })()`,
    8000,
  );
  check("iPad: marker painted into the terminal", painted);
  await sleep(300);

  // The echoed command line also contains the marker; the row that is ONLY the
  // marker is the output line.
  const rect = await evalJs(`(() => {
    for (const row of document.querySelectorAll('.xterm-rows > div')) {
      if (row.textContent.trim() !== ${JSON.stringify(MARKER)}) continue;
      const span = [...row.querySelectorAll('span')].find(s => s.textContent.includes(${JSON.stringify(MARKER)})) ?? row;
      const r = span.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
    }
    return null;
  })()`);
  check("iPad: marker glyphs are addressable", !!rect, JSON.stringify(rect));

  if (rect) {
    // Long-press (> LONG_PRESS_MS 450) selects the word under the finger.
    await S("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: rect.x, y: rect.y }] });
    await sleep(700);
    await S("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await sleep(250);
    check(
      "iPad: long-press selected the word (xterm selection rects painted)",
      (await evalJs("document.querySelectorAll('.xterm-selection div').length")) > 0,
    );

    await click(".term-copy");
    const copied = await clipboard();
    const receipt = await statusText();
    check("iPad: Copy put the SELECTION on the clipboard", copied.trim() === MARKER, copied.trim());
    check("iPad: Copy flashed a receipt", receipt === "Copied", receipt);
  }

  // ---- Paste writes the clipboard into the terminal over the normal input path.
  await setClipboard(`echo PASTED_OK`);
  await click(".term-paste");
  await sleep(600);
  check(
    "iPad: Paste injected the clipboard into the terminal",
    (await termTextDewrapped()).includes("echoPASTED_OK"),
    (await termTextDewrapped()).slice(-60),
  );
  check("iPad: Paste did not need the fallback sheet (secure context)", !(await has(".paste-sheet")));

  // ================================================================ iPad, plain HTTP
  // The real deployment: a tablet on the LAN/relay talks plain HTTP, so it is NOT
  // a secure context and navigator.clipboard is undefined — clipboard READS have
  // no fallback. Paste must then open the PasteSheet instead of no-oping, which
  // is the bug that made the phone's Paste look broken. Simulate by hiding the
  // API, which is exactly what an insecure context does.
  const hideClipboard = await S("Page.addScriptToEvaluateOnNewDocument", {
    source: "Object.defineProperty(navigator, 'clipboard', { get: () => undefined });",
  });
  await openTerminal();
  check(
    "insecure iPad: clipboard reads are unavailable (canReadClipboard() is false)",
    (await evalJs("typeof navigator.clipboard?.readText")) !== "function",
  );
  await click(".term-paste");
  check("insecure iPad: Paste falls back to the PasteSheet", await has(".paste-sheet"));
  check(
    "insecure iPad: the sheet's textarea is focused, so iOS raises its Paste affordance",
    await evalJs("document.activeElement === document.querySelector('.paste-sheet-input')"),
  );
  await S("Page.removeScriptToEvaluateOnNewDocument", { identifier: hideClipboard.identifier });

  // ================================================================ phone
  // The phone already has Copy/Paste on the key bar, docked above the keyboard.
  // A second pair floating over the terminal would be clutter on the smallest screen.
  await emulate({ width: 390, height: 844, coarse: true });
  await openTerminal();
  check("phone: takes the mobile shell", await has(".mobile-shell"));
  check("phone: key bar is present", await has(".term-keybar"));
  check(
    "phone: key bar still carries its own Copy and Paste",
    await evalJs(`(() => { const k = [...document.querySelectorAll('.term-keybar .kb')].map(x=>x.textContent.trim());
      return k.includes('Copy') && k.includes('Paste'); })()`),
  );
  check("phone: no duplicate Copy button over the terminal", !(await has(".term-copy")));
  check("phone: no duplicate Paste button over the terminal", !(await has(".term-paste")));
  check("phone: 📎 attach button is unaffected", await has(".term-attach"));

  // ================================================================ desktop
  // A mouse and a keyboard: ⌘-C / Ctrl-Shift-C already copy, so the buttons
  // would be noise. This is the assertion that keeps the feature off the desktop.
  await emulate({ width: 1280, height: 800, coarse: false });
  await S("Emulation.clearDeviceMetricsOverride");
  await openTerminal();
  check(
    "desktop: (pointer: coarse) does NOT match",
    !(await evalJs("matchMedia('(pointer: coarse)').matches")),
  );
  check("desktop: no Copy button", !(await has(".term-copy")));
  check("desktop: no Paste button", !(await has(".term-paste")));
  check("desktop: 📎 attach button still rendered", await has(".term-attach"));
  check(
    "desktop: 📎 is still clickable (it moved into .term-actions)",
    await evalJs(`(() => {
      const b = document.querySelector('.term-attach');
      const r = b.getBoundingClientRect();
      const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return b.contains(top) || b === top;   // nothing is covering it
    })()`),
  );

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("the terminal's touch Copy/Paste buttons");
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
