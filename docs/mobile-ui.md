# Mobile UI Design

Goal: the full ASM control center on a phone — same features as the desktop web
client, laid out for a narrow touch screen. This is a design + execution plan;
nothing here is implemented yet.

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

## Mobile information architecture

Two screens plus one sheet. The session list is home; the terminal is a
full-screen push; details/git is a sheet **over** the terminal (so the
terminal — and its WebSocket — stays mounted underneath).

```
┌─ Sessions (home) ─┐  tap session   ┌─ Terminal ────────┐  ⓘ   ┌─ Details sheet ─┐
│ health · Daemons  │ ─────────────▶ │ ‹ agent·status ⓘ  │ ───▶ │ VS Code, fields │
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
- Long-press is not required anywhere; every action is already an explicit
  visible button (stop/archive/connect/+/×), which is why the tree ports
  cleanly to touch. Hover-only `title` tooltips degrade silently; all critical
  info (status, attention, path basename, rel-time) is already visible text.

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
  | `Paste` | `clipboard.readText()` → input | long-press paste is flaky in xterm; needs secure context, which the relay path has |

  `TerminalView` grows an optional `onReady(handle)` prop exposing
  `write(data)`/`focus()` so the key bar can inject input through the same
  WS send path as typed keys.

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
the terminal screen, hosting the unchanged `RightPanel`: Continue-in-VS-Code
(kept for parity; on phones the deep link typically fails and the existing
"didn't open" fallback with the copyable CLI command appears), risk banner,
metadata fields, worktree cleanup, end-of-session summary, and the full source
control block — branch, changed files → DiffModal, pull/rebase with branch
picker, commit graph → CommitModal. Rendering it as an overlay (not a pushed
screen) keeps the terminal mounted and attached underneath.

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
| Right panel: VS Code, fields, cleanup, summary | Details sheet, same component |
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

1. **Shell split** — `useIsPhone()`; extract the current 3-pane JSX from
   `App.tsx` into `DesktopShell`; add `MobileShell` (home screen + terminal
   screen + details sheet + navigation state in `useUiStore`:
   `showDetails`, derived screen from `activeSession`). History/pushState
   integration. *(new: MobileShell.tsx, useIsPhone.ts; touch: App.tsx,
   store.ts, main.tsx)*
2. **Touch & sheet CSS** — `PHONE_MQ` media block in `styles.css`: touch
   targets, modal→sheet, safe-area, dvh root. *(touch: styles.css,
   index.html)*
3. **Terminal on touch** — key bar component + `TerminalView` input handle +
   `useVisualViewportHeight`. i18n keys for key labels/tooltips. *(new:
   TermKeyBar.tsx; touch: Terminal.tsx, en.json)*
4. **PWA wrapper** — manifest, icons, theme-color, iOS metas. *(touch:
   index.html; new: public/manifest.webmanifest, icons)*
5. **Verify** — headless-Chrome mobile-viewport pass (390×844 + 844×390)
   driving: connect → new workspace → new session → type via key bar → diff →
   pull → archive; plus desktop regression at 1280×800.

Phases 1–2 make the app usable on a phone; 3 makes the terminal genuinely
workable; 4 is packaging. Each phase ships independently.

## Follow-ups (out of scope, noted so they aren't lost)

- Web Push for `approval_needed`/`likely_blocked` attention states — the
  reason a phone client exists; needs daemon-side push plumbing (relay is the
  natural carrier).
- "Needs attention" pinned group at the top of the home screen (deliberate
  parity break in mobile's favor — glanceability).
- Terminal font-size control (pinch or ± in key bar).
- `title`-tooltip parity on touch (tap-and-hold info popover) if full paths
  turn out to matter on phones.
