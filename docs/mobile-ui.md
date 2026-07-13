# Mobile UI Design

Goal: the full ASM control center on a phone — same features as the desktop web
client, laid out for a narrow touch screen. **Status (2026-07-06): execution
plan phases 1–3 shipped** (adaptive shell, touch/sheet CSS, terminal key bar);
phase 4 (PWA packaging) and the follow-ups remain. See the Execution plan below.

## Constraints

- **Exact feature parity.** Everything the desktop client can do must be
  reachable on the phone. No "lite" mode.
- **One UI per device class, forever.** When native apps ship later, the phone
  app shows the mobile web UI and the iPad app shows the desktop web UI. The
  web client is the single source of truth for both layouts.

## Decision: one codebase, adaptive shell

Three options were considered:

1. **Pure-CSS responsive reflow** — keep the 3-pane grid, let CSS stack it.
   Rejected: a terminal + tree + git panel stacked vertically on a 390 px
   screen is unusable; panel resizers, hover affordances, and the persistent
   topbar don't translate. CSS alone can't express "the terminal becomes its
   own screen you navigate into."
2. **Separate mobile front-end** (second app or route). Rejected: duplicates
   every feature (and every future feature) across two trees — the parity
   constraint makes this a permanent 2× tax, and drift is guaranteed.
3. **Adaptive shell, shared everything else** — one React app; a single
   device-class switch picks between the existing 3-pane `Workspace` shell and
   a new stacked `MobileShell`. `SessionList`, `TerminalView`, `RightPanel`,
   all dialogs, stores, and queries are reused unchanged (plus CSS).
   **Chosen.** Parity is structural: a new feature lands in a shared panel and
   both shells get it.

## Device rule (what counts as "phone")

```
PHONE_MQ = (max-width: 599px), ((max-height: 599px) and (pointer: coarse))
```

- Phones get the mobile shell in **both** orientations (the height clause
  catches landscape phones, where the 3-pane grid technically fits but leaves
  a ~350×300 px terminal under browser chrome + keyboard).
- iPad mini portrait is 744 px logical → always desktop shell, matching the
  "iPad app = desktop web" rule. Desktop windows are never `pointer: coarse`,
  so a short desktop window stays on the desktop shell.
- Implemented as one `useIsPhone()` hook (`matchMedia(PHONE_MQ)` + change
  listener). Crossing the boundary (rotation, resize) swaps shells live; all
  state lives in stores/queries, so nothing is lost.

### Layout class ≠ input class

`PHONE_MQ` answers "how much room is there", **not** "what is this driven with".
A tablet is desktop-*shaped* and finger-*driven*, and anything keyed off
`useIsPhone()` alone silently assumes those two go together. They don't, and the
gap is exactly one device wide: the iPad took the desktop shell (so it never saw
the phone key bar) and had no keyboard (so it could never press ⌘-C / Ctrl-V) —
leaving it, for a while, the one client that could select text but not copy it.

So there is a second, independent hook (`useIsTouch()` = `(pointer: coarse)`) for
input-class questions. `pointer` (not `any-pointer`) describes the **primary**
pointer, which is what makes it the right test: a touchscreen laptop reports a
*fine* primary pointer and keeps its key chords, so it must not be handed
touch-only controls. Reach for `useIsTouch()` whenever the question is "can this
user press a key / hover", and for `useIsPhone()` only when it is "will this fit".

## Mobile information architecture

Two screens plus one sheet. The session list is home; the terminal is a
full-screen push; details/git is a sheet **over** the terminal (so the
terminal — and its WebSocket — stays mounted underneath).

```
┌─ Sessions (home) ─┐  tap session   ┌─ Terminal ────────┐  ⓘ   ┌─ Details sheet ─┐
│ health · Daemons  │ ─────────────▶ │ ‹ agent·status ⓘ  │ ───▶ │ fields, cleanup │
│ host ▸ ws ▸ rows  │ ◀───────────── │  xterm (fills)    │ ◀─── │ SCM, diffs,     │
│ history (bottom)  │  ‹ back /      │ [Esc Tab ^ ⇧⇥ ↑↓] │ swipe│ pull/rebase,    │
└───────────────────┘  system back   └───────────────────┘ down └─ commit graph ──┘
```

### Screen 1 — Sessions (home)

The desktop topbar and left-panel header merge into one compact header row:
title, health dot + `reachable/total · live` counts, **Daemons** (opens
ConnectionDialog) and **+ New** (opens NewSessionDialog). Below it, the
existing host → workspace → session tree and the collapsible History section,
rendered by the same `SessionList` component. Mobile CSS only:

- Rows ≥ 44 px tall (tree nodes today are ~30 px).
- `tree-add` +/× buttons 20 px → 36 px square.
- `btn.tiny` min-height 32 px.
- Long-press is not required anywhere *in the tree*; every action is already an
  explicit visible button (stop/archive/connect/+/×), which is why the tree ports
  cleanly to touch. (The terminal is the one place a long-press carries meaning —
  it starts a selection; see "Touch gestures" below.) Hover-only `title` tooltips
  degrade silently; all critical info (status, attention, path basename,
  rel-time) is already visible text.

Tap a session row → same takeover confirm as desktop → navigates to the
Terminal screen.

### Screen 2 — Terminal

- **Header (44 px):** back chevron, status dot + `agent · host`, then
  **Usage** (when the agent supports it) and **ⓘ Details**.
- **Body:** `TerminalView` fills everything under the header. Same component,
  same WS protocol; the attach-resize already handles phone-sized PTYs (the
  single-attacher model means whoever is attached owns the size — reattaching
  from desktop resizes back; nothing new needed server-side).
- **Key bar** (new, phone-only, live sessions only — hidden for read-only
  replay of ended sessions): a 40 px row docked above the soft keyboard with
  the keys a terminal needs that soft keyboards lack:

  | Key | Sends | Why |
  |---|---|---|
  | `Esc` | `\x1b` | interrupt Claude Code / dismiss TUI prompts |
  | `Tab` | `\x09` | completion |
  | `⇧Tab` | `\x1b[Z` | Claude Code auto-accept toggle |
  | `Ctrl` | latch: next key → `\x01`–`\x1a` | any control chord; double-tap locks |
  | `^C` | `\x03` | dedicated because it's the most common chord |
  | `↑ ↓ ← →` | `\x1b[A/B/C/D` | history, TUI menus |
  | `⌨` | focus xterm textarea | summon/dismiss keyboard (iOS needs a gesture) |
  | `Paste` | `clipboard.readText()` → input, else the paste sheet | long-press paste is flaky in xterm; the read needs a secure context, which a phone does **not** have (below) |

  `TerminalView` grows an optional `onReady(handle)` prop exposing
  `write(data)`/`focus()` so the key bar can inject input through the same
  WS send path as typed keys.

- **Paste without a clipboard read** (fixed 2026-07-12,
  `scripts/mobile-paste-test.mjs`). `navigator.clipboard.readText()` exists only
  in a **secure context**, and the daemon and the relay both serve plain HTTP
  (relay TLS is still open — see `docs/security-followups.md`). So on a phone
  there was no clipboard to read and `Paste` did nothing at all, in silence.

  Two things kept this hidden. A dev machine reaches the client on **localhost,
  which _is_ a secure context** — so the identical code works in Chrome's device
  emulation and fails on the device. And `Copy` went on working next to it,
  because copying falls back to `execCommand("copy")`; only *reading* has no
  fallback (`document.execCommand("paste")` is denied to web content in every
  browser).

  The fix is the one clipboard path that needs neither a secure context nor a
  permission: a **`paste` event** carries its own `clipboardData`, since the OS
  hands the text over precisely because the user chose to paste. So when
  `canReadClipboard()` is false, `Paste` opens `PasteSheet` — a focused textarea
  to paste *into* — and forwards what lands there to the pty. The gesture stays
  the platform's own (iOS: long-press → Paste). The check must be **synchronous**
  inside the click handler: `await` first and the user gesture is spent, and iOS
  will not raise the keyboard for the sheet.

  When the relay does get TLS, the read path lights up on its own and the sheet
  becomes the fallback it was written to be.

- **Touch gestures** (added 2026-07-12, `scripts/touch-select-test.mjs`):

  | Gesture | Does |
  |---|---|
  | Drag | Scrolls — the same from anywhere, over text or over blank space |
  | Long-press (450 ms) | Selects the word under the finger (short vibrate) |
  | Drag, still held | Extends the selection cell-by-cell, forward or backward; holding near the top/bottom edge auto-scrolls so a selection can outrun one screenful |
  | Tap | Dismisses the selection |

  Then `Copy` on the key bar puts it on the clipboard — or, on a tablet, the
  `⧉` button below.

- **Clipboard buttons on the terminal** (added 2026-07-13,
  `scripts/touch-clipboard-test.mjs`): the terminal's bottom-right action row is
  `[⧉ copy] [📋 paste] [📎 attach]`, with the clipboard pair rendered only when
  `useIsTouch() && !useIsPhone()`.

  That predicate is the whole feature. A phone already has Copy/Paste on the key
  bar docked above its keyboard, and a mouse user has ⌘-C / Ctrl-Shift-C; both
  would only be cluttered by a second pair. What was left over was the tablet —
  desktop shell, no key bar, no keyboard — which had **no way to copy or paste at
  all**, even though the long-press selection gesture above worked there the whole
  time. The buttons reuse the paths that already existed (`useTerminalPaste()` is
  shared with the key bar, so the secure-context dance is written once; `Copy`
  flashes the same `.paste-status` receipt as Ctrl-Shift-C).

  `Copy` with an empty selection says *"Select some text first"* rather than
  no-oping: it is the one copy path reachable without a selection (the key chords
  and the context menu all guard on `hasSelection()`), and on a tablet — where
  selecting means a long-press drag that is easy to miss — a button that does
  nothing at all reads as a broken button.

  **Testing gotcha.** Chrome's `Emulation.setEmulatedMedia` does **not** support
  the `pointer` media feature — it silently no-ops, and every touch assertion
  passes for the wrong reason. Only `setDeviceMetricsOverride({mobile: true})` +
  `setTouchEmulationEnabled` actually flips `(pointer: coarse)`, so the test
  asserts the media query itself before trusting anything downstream.

  Two things make this more than "listen for `touchmove`":

  1. **xterm has no touch selection at all.** Its selection service is
     mouse-only and `.xterm` carries `user-select: none`, so neither xterm nor
     the browser will select a cell from a fingertip. The gesture is synthesized
     and pushed through xterm's *own* selection model via `term.select()` —
     which is why every existing copy path (the key bar's `Copy`,
     `getSelection()`, right-click, Ctrl-Shift-C) works on it unchanged, and why
     the selection is cell-accurate rather than scraped out of the DOM.
  2. **The gesture rides on pointer events, not touch events.** The DOM renderer
     *replaces* a row's `<span>`s when that row repaints. A touch whose
     `touchstart` landed on one of those spans gets retargeted to a node that is
     no longer in the tree, so its `touchmove`s silently stop reaching any
     listener — no `touchcancel`, just nothing. Since scrolling repaints rows,
     a drag beginning on **text** scrolled exactly one row and then died, while a
     drag beginning on **blank space** (whose target is the row `<div>` — which
     xterm recycles rather than replaces) scrolled normally. `setPointerCapture`
     pins the gesture to the container, so what becomes of the element under the
     finger stops mattering.

  **Capture is best-effort, and the selection never depends on it.** It is only
  defined for an *active* pointer, and the long-press timer fires outside any
  pointer event handler — Blink allows a capture from there, other engines need
  not, and an exception used to escape `beginSelect()` **before** it selected
  anything. So the word is selected first, the capture is attempted in a
  `try`/`catch`, and the next `pointermove` retries it from a handler no engine
  objects to.

  **Debugging this on a real phone.** `scripts/touch-select-test.mjs` drives
  headless Chrome, and Chrome's device-emulation mode is not a phone: it
  synthesizes pointer events from a mouse and runs none of a real device's
  gesture recognizers, so it will pass a gesture that iOS kills. An iPhone is
  also the one browser we cannot instrument from Linux (Safari's Web Inspector
  needs a Mac). So load the client with **`?gesturelog=1`** (`gestureLog.ts`) and
  the raw event stream plus the gesture layer's own decisions paint onto an
  overlay you can read — and copy — on the device:

  ```
     0 pointerdown touch#2 37,99 span conn=1     ← do pointer events arrive at all?
   452 ▸select 0,2 "SELECTME"                    ← did the long press land (450 ms)?
   716 ▸up gesture=select
  ```

  What to look for: a `pointercancel` **before** 450 ms is the engine claiming
  the touch for a gesture of its own; `conn=0` is the renderer detaching the
  target out from under the finger; `▸capture-failed` names a refused
  `setPointerCapture`; no overlay at all means the phone is running a stale
  bundle.

- **Soft-keyboard geometry:** viewport meta gains
  `interactive-widget=resizes-content` (Android); an `useVisualViewportHeight`
  hook drives the shell height on iOS so the key bar sits exactly above the
  keyboard and the terminal refits (the existing `ResizeObserver` → `fit()` →
  resize-message chain does the rest). Root uses `100dvh` + safe-area insets.
- **Back** (header chevron, browser/system back): clears the active session →
  detaches the WS. That's the intended model — "client connections are
  temporary views into sessions"; reattach replays the server-side snapshot.

### Sheet — Details (RightPanel)

Full-height sheet (~94 dvh, drag-handle + swipe-down/× to close) sliding over
the terminal screen, hosting the `RightPanel`: risk banner, metadata fields,
worktree cleanup, end-of-session summary, and the full source control block —
branch, changed files → DiffModal, pull/rebase with branch picker, commit graph
→ CommitModal. Rendering it as an overlay (not a pushed screen) keeps the
terminal mounted and attached underneath.

**Continue-in-VS-Code is the one deliberate parity break:** the whole affordance
is hidden on phones (`RightPanel` gates it behind `!useIsPhone()`). A phone has
no local VS Code for the `vscode://` deep link to reach, so the button and its
"didn't open"/CLI fallback are dead weight; the browser web editor (V-track)
will be the mobile editing path.

### Modals → full-screen sheets

All existing dialogs keep their components; under `PHONE_MQ` the `.modal` CSS
becomes a bottom sheet: `width: 100vw; max-width: none; height: min(94dvh,
fit)`, radius only on top, internal scroll, actions sticky at the bottom, top
inset respects the notch. Specifics:

- **NewSessionDialog** (longest form) — full-height sheet, scrollable.
- **DirectoryPicker** — sheet over the sheet; list gets 44 px rows.
- **DiffModal / CommitModal** — full-height; `.diff-view` keeps `pre`
  whitespace and scrolls horizontally inside itself (never the page), font
  drops to 11 px.
- **ConnectionDialog / UsageModal / NewWorkspaceDialog** — sheet, no other
  change.

### Navigation & deep links

Opening a session pushes `#s=<daemonId>:<sessionId>` via `history.pushState`;
the details sheet and full-screen dialogs each push a state too, so the
Android back gesture / iOS edge-swipe closes the top-most layer instead of
leaving the app. Bonus: the hash is a shareable deep link that also works on
desktop (select session on load).

## Feature-parity map

| Desktop | Phone |
|---|---|
| Topbar health summary + Manage | Home header (dot + counts, Daemons btn) |
| Session tree: connect/disconnect, badges, +ws, +session, remove ws | Home screen, identical tree, bigger targets |
| History section (collapsed sessions) | Home screen bottom, identical |
| Stop / archive / takeover confirm / attention ack | Identical (row buttons + row tap) |
| Terminal + status header + View usage | Terminal screen + header buttons |
| Keyboard input incl. Esc/Ctrl/arrows | Soft keyboard + key bar (new) |
| Right panel: VS Code, fields, cleanup, summary | Details sheet, same component (VS Code hidden — see below) |
| SCM: status, changed files, diff, pull, rebase, commit graph, commit detail | Details sheet, same components; modals as sheets |
| New session / new workspace / directory picker / connection & relay manage / usage | Same dialogs as full-screen sheets |
| Panel resize | N/A on phone (no panels) — desktop unchanged |

## Packaging path for the future apps

- Add a web-app manifest (`display: standalone`, `background_color/theme_color
  #0b0e14`, icons) + iOS meta tags now: "Add to Home Screen" becomes the
  zero-cost phone app immediately, and the eventual store apps are thin
  WebView/Capacitor wrappers around the same origin — phone wrapper renders
  the mobile shell, iPad wrapper renders the desktop shell purely by viewport
  size. No per-platform UI work ever.
- Service worker / offline shell is deliberately out of scope (app is
  meaningless offline; adds cache-invalidation risk during rapid iteration).

## Execution plan

1. ✅ **Shell split** (landed 2026-07-06) — `useIsPhone()`; extracted the 3-pane
   JSX from `App.tsx` into `DesktopShell`; added `MobileShell` (home + terminal
   + details sheet). Nav state in `useUiStore` (`showDetails`) + browser-history
   pushState mirroring the layer stack + `#s=` deep-link. *(new: MobileShell.tsx,
   useIsPhone.ts, agents.ts, DesktopShell.tsx; touch: App.tsx, store.ts)* — note
   the shared dialogs live in `App.tsx`, not `main.tsx`.
2. ✅ **Touch & sheet CSS** (landed 2026-07-06) — one `@media PHONE_MQ` block in
   `styles.css`: touch targets, modal→bottom-sheet, safe-area, 100dvh root;
   `viewport-fit=cover`. *(touch: styles.css, index.html)*
3. ✅ **Terminal on touch** (landed 2026-07-06) — `TermKeyBar` +
   `TerminalView` input handle (write/focus/getSelection) + Ctrl latch +
   `useVisualViewportHeight` + `interactive-widget=resizes-content`. i18n
   `keybar.*`. *(new: TermKeyBar.tsx, terminalTypes.ts, useVisualViewportHeight.ts;
   touch: Terminal.tsx, MobileShell.tsx, clipboard.ts, en.json, index.html)*
4. **PWA wrapper** — manifest, icons, theme-color, iOS metas. *(touch:
   index.html; new: public/manifest.webmanifest, icons)*
5. ✅ **Verify** (for phases 1–3) — `scripts/mobile-shell-test.mjs`: headless
   Chrome at 390×844 against a live shell session drives device switch → session
   tap → key-bar/Ctrl-latch input over the WS → details sheet → back navigation
   (ALL PASS), plus desktop regression at 1280×800. Re-run after phase 4.

Phases 1–2 make the app usable on a phone; 3 makes the terminal genuinely
workable (all shipped); 4 is packaging. Each phase ships independently.

## Follow-ups (out of scope, noted so they aren't lost)

- Web Push for `approval_needed`/`likely_blocked` attention states — the
  reason a phone client exists; needs daemon-side push plumbing (relay is the
  natural carrier).
- "Needs attention" pinned group at the top of the home screen (deliberate
  parity break in mobile's favor — glanceability).
- Terminal font-size control (pinch or ± in key bar).
- `title`-tooltip parity on touch (tap-and-hold info popover) if full paths
  turn out to matter on phones.
