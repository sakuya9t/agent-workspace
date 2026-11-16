# Terminal history limit and "Clear history"

Status: **implemented 2026-11-15** (backlog **TERM-HISTORY**). Client + daemon.
Test: `scripts/term-history-test.mjs` (sandboxed, daemon + headless Chrome).

## Request

> In the session window, there should be a button to clean up too much history
> text in the mux window. If the conversation is very long, the page can be
> lagging or even die. Should have a maximum displayed characters set to prevent
> that. But when exporting conversation, we should include everything, including
> the trimmed words in the mux window.

## What was actually unbounded (measured 2026-11-15, live install)

History reaches a browser terminal by three paths. Only one was uncapped.

| Path | Before | Bound |
| --- | --- | --- |
| Live attach, normal buffer (codex, shell): raw ring replay | ≤ 8 MB | `HISTORY_RING_BYTES` |
| Live attach, alternate screen (claude `tui: fullscreen`, vim) | one screen | the app owns its history |
| Live streaming into xterm | 5,000 lines | xterm `scrollback` |
| **Opening an ended session** (`handle_history`) | **the whole record** | **none** |

Ended claude sessions on the live install replayed **20–44 MB** in one WS frame;
archived codex sessions had reached **900 MB** of recorded output. xterm.js parses
a frame like that for many seconds on the main thread, and past 50 MB it discards
the excess ("write data discarded"), so the tab stalls, shows garbage, or dies.

The example the user named, session `688f174f` (claude, 5 days, 40 MB recorded,
39 MB transcript), is a **fullscreen claude** session: its browser terminal only
ever holds one 77×220 screen, and the attach sends just that screen. Its lag had
other sources, found while investigating:

- the host was at load ~90 on 32 cores, mostly that session's own parallel
  training jobs, which starves claude's own redraws;
- its Details panel polled 374 KB of SCM status every 2.5 s and re-rendered
  **2,733 changed-file rows** (2,723 untracked `runs/` outputs) each time.

The second one is fixed here too (below). The first is outside ASM.

## Design

### One knob: a per-browser history limit, in characters

`client/src/terminalPrefs.ts`. Settings → **This browser** → **Terminal history**:
250K / 500K / **1M (default)** / 2M / 5M characters, in `localStorage`
(`asm.terminalHistoryChars`), applied on change. Per browser, not per host:
what a page can comfortably hold is a property of the device (a phone wants less
than a desktop looking at the same session).

It bounds two things:

1. **What the terminal keeps.** xterm's `scrollback` = ⌈limit ÷ cols⌉ lines,
   clamped to [200, 50,000], re-derived on every width change and whenever the
   setting changes. xterm stores a fixed number of cells per line, so this bounds
   both the text kept and the memory behind it; lowering the limit trims a full
   terminal on the spot. At 220 columns the default is ≈ the old 5,000 lines.
2. **What an attach replays.** The stream socket carries `?replay=<bytes>` =
   limit × 8. A raw stream is far more than its text (escape codes, colors, TUI
   redraws — codex spends ~4 KB of stream per line that survives into
   scrollback), so the replay is budgeted separately from what stays on screen;
   at 8 B/char the 1M default asks for exactly 8 MB, the whole ring, so the
   default changes nothing for live attaches. The daemon clamps the request to
   `[REPLAY_MIN_BYTES = 64 KB, HISTORY_RING_BYTES = 8 MB]` (`replay_budget`).

Daemon side (`backend/mod.rs`, `api/ws.rs`, `db.rs`):

- **Live, normal buffer:** `raw_history_repaint` replays only the ring's newest
  whole chunks within the budget (`HistoryRing::extend_tail_into`). A cut lands
  on a chunk boundary, exactly like a ring eviction, so it degrades the same way.
- **Ended session:** `handle_history` reads only the newest events within the
  budget (`Db::read_events_tail`). When older output is left out, the frame opens
  with the raw-replay preamble and a dim notice: `[asm: N MB of earlier output is
  not shown here. Save conversation exports all of it.]`
- **Alternate screen:** unchanged — one rendered screen, budget ignored.

### "Clear history" (🧹)

The terminal's corner actions (desktop mouse + phone) and the iPad control panel.
Live sessions only.

`POST /api/sessions/:id/clear-history` → `BackendSession::clear_history` →
`backend::clear_history`: under the parser lock, then the ring lock, the ring is
reset to a single seed — a history-free repaint of the current screen (active
buffer, contents, cursor, input modes; `screen_repaint`). The client then
**re-attaches**; the fresh snapshot is preamble + seed + whatever arrived since,
so the clear holds across reconnects, reloads and other devices.

Why re-attach rather than erase in place: an erase (`ESC[3J`) injected into the
client's parser mid-stream can land inside an escape sequence that a PTY read
split, and the sequence's tail then prints as text. The new socket's first frame
is whole, and its preamble starts from a fresh `ESC`.

Lock order is the point of the sidecar change: `drain_loop` now pushes the ring
*inside* the parser lock (native `reader_loop` already did). Parser → ring is the
order attach and clear take them, so a chunk is either already in the seeded
screen or arrives after it, never both.

While a full-screen app holds the alternate buffer the button is dimmed and a tap
says *"This app keeps its own history, so the terminal has none to clear."*
That's the claude `tui: fullscreen` case: claude holds the conversation in its
own process, and the browser has nothing to drop.

Known limit: a **daemon restart** re-seeds the ring from the recorded tail
(`seed_from_cold`), so cleared history can come back after one, still within the
replay budget. Persisting a clear point across restarts would be an M4-C
cold-stitch concern; not done.

### Export stays complete

Neither the cap nor the clear touches what "Save conversation" reads:
`GET /api/sessions/:id/transcript` renders the agent's own transcript (claude /
codex JSONL), or returns the **full** `terminal_events` record for agents with no
transcript (a shell). Both only grow. The e2e test asserts the export still holds
the cleared and trimmed lines, live and after the session ends.

### Details panel: cap the changed-files list

`GET /api/sessions/:id/scm/status` lists at most `MAX_STATUS_FILES = 200`
changed files, tracked changes before untracked ones (so a pile of new outputs
never hides an edit), and adds `changed_files_total`. The panel shows a footer
row: *"N more changed files aren't listed (M in all)."* Diff, discard and commit
are unaffected: discard re-resolves the path on the daemon, and commit is driven
by the agent.

## Verification (2026-11-15)

- `cargo test -p asm-daemon`: 334 pass, including ring-tail, replay-cap,
  clear-history (screen + cursor + modes kept, scrollback 0, later output still
  appended), budget clamp, and changed-files cap units.
- `node scripts/term-history-test.mjs`: 40/40. Daemon: default attach 340 KB
  with `hist-001`; `?replay=65536` → 65 KB, newest kept, oldest gone; clear →
  1.4 KB one-screen attach, later output still streams and replays; export still
  has the cleared lines; ended + capped → preamble + marker + tail; clear on an
  ended session → 409; status → 200 of 251, tracked edit first. Browser: 🧹
  shrinks 2,002 → 45 lines (one screen) and survives a reload; the 250K setting
  is saved and caps scrollback at 2,578 lines + screen at 97 columns; alternate
  screen dims the button and explains; the panel footer reads "51 more changed
  files aren't listed (251 in all)".
