// Headless-Chrome verification of TOUCH text selection in the terminal, at a
// phone viewport with touch emulation on. Guards the three-way standoff that
// made "long press to select" do nothing on a real phone:
//
//   1. xterm.css sets `user-select: none` on `.xterm`, so the browser refuses to
//      select anything inside the terminal — xterm ships its own MOUSE-only
//      selection service and has no touch selection at all.
//   2. `.terminal-mount { touch-action: none }` suppressed the browser's own
//      touch gestures over the grid.
//   3. TerminalView's capture-phase `touchmove` handler preventDefault()s every
//      drag past a 6px slop to turn it into scrollback wheel events — so a drag
//      could never become a selection either.
//
// The fix makes the browser's NATIVE selection the touch gesture (long-press →
// OS handles + Copy bubble) while keeping drag = scroll. So this asserts both,
// plus that the key bar's Copy reads the native selection.
//
//   cd client && npm run build            # once, to produce client/dist
//   node scripts/touch-select-test.mjs    # sandboxed: daemon + chrome + session
//
// Same-origin loopback ⇒ no token. Node 18+ (global fetch/WS). See also
// docs/mobile-ui.md and scripts/mobile-shell-test.mjs.

import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-tsel");

// A marker with no repeats and no shell metacharacters, echoed into the shell so
// we have a known run of glyphs to aim a fingertip at.
const MARKER = "SELECTME";

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

  // Phone: 390×844 matches PHONE_MQ's (max-width:599px); touch emulation makes
  // (pointer: coarse) match, which is what gates the selection CSS.
  await S("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 3,
    mobile: true,
  });
  await S("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  await S("Network.setCacheDisabled", { cacheDisabled: true }); // else a stale index.html silently runs the OLD bundle
  // navigator.clipboard refuses to read/write unless the document is focused,
  // and a headless page isn't until something real lands on it — so force it,
  // or the key-bar Copy assertions fail for a reason that has nothing to do
  // with the selection.
  await S("Emulation.setFocusEmulationEnabled", { enabled: true });
  await S("Page.navigate", { url: `${sb.http}/` });

  await waitFor("!!document.querySelector('.mobile-shell')");
  await waitFor("!!document.querySelector('.session-row')");
  await evalJs("document.querySelector('.session-row').click()");
  check("mobile terminal screen mounted", await waitFor("!!document.querySelector('.terminal-host')"));
  await waitFor(
    "!document.querySelector('.terminal-loading') && (document.querySelector('.terminal-mount .xterm-rows')?.innerText||'').trim().length > 0",
  );

  check(
    "(pointer: coarse) matches — the selection CSS is in scope",
    await evalJs("window.matchMedia('(pointer: coarse)').matches"),
  );

  // --- Paint a known marker, and fill scrollback so scroll has somewhere to go.
  // A "\n" inside Input.insertText does NOT submit — the shell just echoes the
  // line and waits. Enter has to be a real key event.
  const enter = async () => {
    await S("Input.dispatchKeyEvent", {
      type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r",
    });
    await S("Input.dispatchKeyEvent", {
      type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13,
    });
  };
  await evalJs("document.querySelector('.xterm-helper-textarea').focus()");
  await S("Input.insertText", { text: `seq 1 120; echo ${MARKER}` });
  await enter();
  // The command line itself echoes the marker, so wait for the *output* copy:
  // the run is done once the shell has printed line 120 below it.
  const seenMarker = await waitFor(
    `(() => { const t = document.querySelector('.terminal-host').innerText;
       return /^120$/m.test(t) && /^${MARKER}$/m.test(t); })()`,
    8000,
  );
  check("marker + 120 lines of output painted into the terminal", seenMarker);
  await sleep(400);

  // Locate the marker's glyphs in the DOM renderer's rows (no canvas/WebGL addon
  // is loaded, so the cells are real spans) and hand back their centre.
  // The echoed command line also contains the marker, so match the row that is
  // ONLY the marker — that is the output line, and a lone word is the cleanest
  // long-press target.
  const markerRect = await evalJs(`(() => {
    for (const row of document.querySelectorAll('.xterm-rows > div')) {
      if (row.textContent.trim() !== ${JSON.stringify(MARKER)}) continue;
      const span = [...row.querySelectorAll('span')].find(s => s.textContent.includes(${JSON.stringify(MARKER)})) ?? row;
      const r = span.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
    }
    return null;
  })()`);
  check("marker glyphs are addressable DOM cells", !!markerRect, JSON.stringify(markerRect));
  if (!markerRect) return report("touch selection");

  // The gesture drives xterm's OWN selection model (term.select), not a DOM
  // Range, so window.getSelection() stays empty by design. xterm mirrors the
  // model into the DOM as .xterm-selection rects — count them to see the
  // selection's shape, and click the key bar's Copy to assert its text.
  const selRects = () =>
    evalJs("document.querySelectorAll('.xterm-selection div').length");
  // A tap anywhere in the grid dismisses a selection (see onPointerUp).
  const tapAway = async () => {
    await S("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: 200, y: 300 }] });
    await S("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await sleep(200);
  };
  const setClipboard = (t) =>
    evalJs(`navigator.clipboard.writeText(${JSON.stringify(t)}).then(() => true).catch(() => false)`);
  const clipboard = () => evalJs("navigator.clipboard.readText().catch(() => '')");
  const copyViaKeyBar = async () => {
    await evalJs(
      "[...document.querySelectorAll('.term-keybar .kb')].find(x=>x.textContent.trim()==='Copy')?.click()",
    );
    await sleep(400);
    return clipboard();
  };
  // xterm's Viewport keeps `.xterm-viewport`'s scrollTop synced to the buffer's
  // scroll position, so it reads the scrollback offset without a prod test hook.
  const SCROLL_TOP = "document.querySelector('.xterm-viewport').scrollTop";
  const toBottom = async () => {
    await evalJs(`(() => { const v = document.querySelector('.xterm-viewport');
      v.scrollTop = v.scrollHeight; })(); true`);
    await sleep(250);
  };

  // ---------------------------------------------------------------- T1
  // Long-press over text selects the word under the finger. This is THE gesture
  // the bug report was about ("long press doesn't work, nothing got selected").
  const press = async (x, y, holdMs = 700) => {
    await S("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y }] });
    await sleep(holdMs); // past LONG_PRESS_MS (450), finger still
  };
  const lift = async () => {
    await S("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await sleep(250);
  };

  await setClipboard("__NOTHING_COPIED__"); // so a no-op Copy can't pass on a stale clipboard
  await press(markerRect.x, markerRect.y);
  const rectsDuringPress = await selRects();
  await lift();
  check(
    "T1 long-press paints a selection over the word",
    rectsDuringPress > 0,
    `${rectsDuringPress} selection rect(s)`,
  );

  // ---------------------------------------------------------------- T2
  // The key bar's Copy reads TerminalHandle.getSelection() → term.getSelection().
  // Because the gesture drives xterm's own selection model, that path needed no
  // change — so this proves the whole chain: gesture → model → clipboard.
  const clip = await copyViaKeyBar();
  check(
    "T2 key-bar Copy puts the long-pressed word on the clipboard",
    String(clip).trim() === MARKER,
    `clipboard=${JSON.stringify(String(clip).slice(0, 60))}`,
  );

  // ---------------------------------------------------------------- T2b
  // Rule 2: "dragging the selected block should select more." Long-press to
  // anchor a word, then drag DOWN without lifting — the selection must grow to
  // cover the rows in between, not just re-select the word under the finger.
  await tapAway();
  await toBottom();
  await setClipboard("__NOTHING_COPIED__");
  const anchorRect = await evalJs(`(() => {
    for (const row of document.querySelectorAll('.xterm-rows > div')) {
      if (row.textContent.trim() !== '100') continue;   // a line from \`seq 1 120\`
      const r = row.getBoundingClientRect();
      return { x: Math.round(r.left + 8), y: Math.round(r.top + r.height / 2), h: Math.round(r.height) };
    }
    return null;
  })()`);
  check("drag anchor row located", !!anchorRect, JSON.stringify(anchorRect));
  if (!anchorRect) return report("touch selection");

  await press(anchorRect.x, anchorRect.y);
  const rectsBeforeDrag = await selRects();
  for (let i = 1; i <= 5; i++) {
    // Still holding: drag down a row at a time.
    await S("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [{ x: anchorRect.x + 20, y: anchorRect.y + i * anchorRect.h }],
    });
    await sleep(50);
  }
  const rectsAfterDrag = await selRects();
  await lift();
  check(
    "T2b drag after long-press extends the selection",
    rectsAfterDrag > rectsBeforeDrag,
    `selection rects ${rectsBeforeDrag} → ${rectsAfterDrag}`,
  );

  const dragClip = await copyViaKeyBar();
  const lines = String(dragClip).split("\n").map((l) => l.trim()).filter(Boolean);
  check(
    "T2b the extended selection copies every row it grew over",
    lines.length >= 5 && lines[0] === "100" && lines[lines.length - 1] === "105",
    `${lines.length} line(s): ${JSON.stringify(lines.slice(0, 8))}`,
  );

  // ---------------------------------------------------------------- T2c
  // Holding an extend-drag against an edge auto-scrolls, so a selection can run
  // past one screenful. The real case: sitting at the prompt, anchor near the
  // bottom and drag UP to reach output that has scrolled off. Park the finger on
  // the top edge and stop moving — the edge timer, not a touchMove, does the rest.
  await tapAway();
  await toBottom();
  await setClipboard("__NOTHING_COPIED__");
  const topY = await evalJs(
    "Math.round(document.querySelector('.xterm-screen').getBoundingClientRect().top + 4)",
  );
  const scrollBefore = await evalJs(SCROLL_TOP);
  await press(markerRect.x, markerRect.y); // near the bottom of the screen
  await S("Input.dispatchTouchEvent", {
    type: "touchMove",
    touchPoints: [{ x: markerRect.x, y: topY }],
  });
  await sleep(900); // finger parked on the edge — the auto-scroll timer runs
  const scrollAfter = await evalJs(SCROLL_TOP);
  await lift();
  check(
    "T2c holding an extend-drag at the top edge auto-scrolls",
    scrollAfter < scrollBefore,
    `scrollTop ${scrollBefore} → ${scrollAfter}`,
  );
  const edgeClip = await copyViaKeyBar();
  const edgeLines = String(edgeClip).split("\n").map((l) => l.trim()).filter(Boolean);
  const visibleRows = await evalJs("document.querySelectorAll('.xterm-rows > div').length");
  check(
    "T2c the auto-scrolled selection runs past one screenful",
    edgeLines.length > visibleRows,
    `${edgeLines.length} line(s) selected, screen holds ${visibleRows}`,
  );

  // ---------------------------------------------------------------- T3
  // Rule 1: with no long-press, "touch anywhere should give the same scrolling
  // experience". The reported symptom was that a drag starting on TEXT scrolled
  // only a hair (the renderer replaced the <span> the touch had latched onto,
  // silently starving the listener) while one starting on blank space scrolled
  // normally. Pointer capture makes the two identical.
  await tapAway();
  const ydiff = async (drag) => {
    const before = await evalJs(SCROLL_TOP);
    await drag();
    await sleep(350);
    const after = await evalJs(SCROLL_TOP);
    return before - after; // finger down ⇒ scroll toward older ⇒ scrollTop drops
  };
  const swipeDown = (x, y0) => async () => {
    await S("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y: y0 }] });
    for (let i = 1; i <= 10; i++) {
      await S("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: [{ x, y: y0 + i * 18 }],
      });
      await sleep(16);
    }
    await S("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  };

  await toBottom();
  const overText = await ydiff(swipeDown(markerRect.x, markerRect.y));
  check("T3 drag over TEXT scrolls the scrollback", overText > 0, `scrollTop moved ${overText}px`);

  // Blank space, well right of any glyphs on the marker row — the case the user
  // said already worked, kept honest so the fix doesn't regress it.
  const blankX = await evalJs(
    "Math.round(document.querySelector('.xterm-screen').getBoundingClientRect().right - 12)",
  );
  await toBottom();
  const overBlank = await ydiff(swipeDown(blankX, markerRect.y));
  check("T3 drag over BLANK space scrolls the same way", overBlank > 0, `scrollTop moved ${overBlank}px`);
  check(
    "T3 text-drag and blank-drag scroll the same distance",
    Math.abs(overText - overBlank) <= 2,
    `text=${overText} blank=${overBlank}`,
  );

  // ---------------------------------------------------------------- T4
  // A plain tap must still dismiss the selection and focus the terminal, and
  // typing must still reach the pty — the gesture layer swallows some synthetic
  // mouse events, so prove it didn't swallow the tap-to-focus path with them.
  await toBottom();
  await press(markerRect.x, markerRect.y); // long-press → selection
  await lift();
  check("T4 a long-press leaves a selection behind", (await selRects()) > 0);
  await tapAway(); // plain tap → dismissed
  check("T4 a plain tap dismisses the selection", (await selRects()) === 0);

  await evalJs("document.querySelector('.xterm-helper-textarea').focus()");
  await S("Input.insertText", { text: "echo TYPED_OK" });
  await enter();
  check(
    "T4 typing still reaches the pty after a touch selection",
    await waitFor(`/^TYPED_OK$/m.test(document.querySelector('.terminal-host').innerText)`, 8000),
  );

  await chrome.send("Target.closeTarget", { targetId: page.targetId }).catch(() => {});
  chrome.ws.close();
  return report("terminal text is selectable by touch, and drag still scrolls");
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
