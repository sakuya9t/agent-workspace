# Terminal scrollback on attach — codex (normal-buffer agents)

Status: **implemented 2026-07-08** (backlog **TERM-SCROLL**, P1). Designed
2026-07-07 from a diagnosis of the reported bug: *"For claude the TUI scrolling
works fine; for codex it only scrolls a few lines above the top — it cannot reach
the start of the conversation."* As-built + verification at the end of this doc.

## Symptom

Attach (or reconnect) to a running **codex** session in the web client, scroll
the wheel up: you see a handful of lines above the viewport, then it stops well
short of the conversation start. **claude** in the same client scrolls all the
way back. Only the two agents' *rendering styles* differ — both run through the
identical native-PTY + `vt100` + xterm.js path (`backend/native.rs`), with no
per-agent branching in the capture or transport layers.

## Root cause

Codex and Claude drive the terminal in fundamentally different ways. Measured
from the real recorded byte streams in `terminal_events`:

| | Claude | Codex |
| --- | --- | --- |
| Alternate screen (`DECSET 1049`) | **yes** | no — normal buffer |
| Mouse reporting (`?1000/1002/1003/1006`) | **yes** (re-asserted every redraw) | no |
| Scroll region (`DECSTBM`, `ESC[t;br`) | none | **`ESC[1;47r`** (55×), `ESC[r` reset (95×) — a **bottom margin**, reserving the lower rows for its composer |
| Reverse index / scroll-up | none | `ESC M` (66×), `ESC[nS` (8×) |
| Who owns "scroll back through history" | **the app itself** | **the terminal's native scrollback** |

**Claude** is an alternate-screen app that owns its own scrolling: the attach
repaint re-arms `?1049h` + the mouse modes, so the client forwards the wheel to
Claude as mouse reports and Claude redraws the visible window from its own
in-memory transcript. It never depends on terminal scrollback — so it is
unaffected by the bug and **must keep working unchanged.**

**Codex** renders its transcript in the *normal buffer* inside a `DECSTBM`
scroll region whose **bottom margin is above the last row** (`ESC[1;47r` on a
54-row screen — the bottom ~7 rows are its composer/status). It enables neither
the alternate screen nor mouse reporting, so it relies on the terminal's native
scrollback to hold history and on the terminal to handle the wheel.

The defect is in the **server-side emulator**. The `vt100` 0.15 crate does
**not** push lines scrolled off the *top* of a region that has a bottom margin
into scrollback. Real terminals and the client's own xterm.js **do**. Fed the
identical byte stream:

| Emulator | Controlled `ESC[1;4r`, 20 lines, 6-row screen | Real codex stream (3.8 MB) |
| --- | --- | --- |
| **tmux** (real-terminal semantics) | 20 → scrollback | ~994 lines |
| **xterm.js 5.5** (the ASM client) | 17 → scrollback | **907 lines** |
| **`vt100` 0.15** (daemon snapshot emulator) | **0** | **0** |

(A full-height region `ESC[1;6r`, or no region, gives `vt100` the expected 15 —
the suppression is specifically the sub-screen *bottom margin*.)

The attach snapshot is built from `vt100`'s scrollback:
`backend::repaint_with_history` (`backend/mod.rs:84`) reads
`parser.screen().scrollback()`, which is **0** for codex, so it degrades to a
screen-only repaint — exactly the alt-screen fallback the function documents,
reached here for a *different* reason. `handle_live` (`api/ws.rs:117`) sends that
history-less repaint, then streams live. So a freshly-attached client starts
with an empty scrollback and can only accumulate the few lines codex
forward-scrolls *after* the attach. On reconnect it is worse: the client clears
the normal-buffer scrollback it had (`Terminal.tsx:276`, deliberately, to avoid
double-appending the repaint) and the repaint gives nothing back.

A client that watched the whole session **live from the start** does scroll
correctly — xterm.js accumulates the full ~907 lines itself. The failure is
specific to the **attach/reconnect snapshot**, which is the common case (open an
already-running codex tab; reload; network blip; takeover).

### Why not just read history from SQLite on attach

`terminal_events` holds every raw byte and `db().read_events_after()` already
raw-replays it for **exited** sessions (`api/ws.rs:209`, `handle_history`). But
`EventSink::send` (`db.rs:41`) only pushes to an **unbounded channel** drained by
a separate writer thread — persistence is **asynchronous** to the broadcast. For
a *live* attach the DB can lag the live stream, so the recent tail may be missing
or duplicated relative to the `broadcast` subscription. The consistent history
source for a live attach must be **in-memory**, captured under the same lock as
the broadcast and the `seq` counter.

## Fix — a per-buffer-model attach strategy (decoupled per provider)

Keep the two rendering models on separate, independent paths so codex's fix
cannot regress claude.

1. **Alternate-screen / self-scrolling apps (claude, or any TUI holding
   `?1049`) → unchanged.** Current `repaint_with_history`: arm `?1049h`, replay
   the screen + input modes, no history (the app self-scrolls). Untouched.

2. **Normal-buffer / terminal-scrollback apps (codex, shell, …) → raw-history
   replay.** Build the attach payload from a bounded **in-memory raw-byte ring**
   (whole PTY read-chunks, prefixed with a mode-normalizing preamble) instead of
   `vt100`'s scrollback. The client's xterm.js rebuilds scrollback **and** screen
   exactly — verified: replaying the full recorded stream through xterm.js yields
   the true conversation start (codex banner + first user prompt) as the oldest
   scrollback line, continuous into the live viewport.

   Selection keys off the **runtime buffer state** (`screen().alternate_screen()`
   — the branch `repaint_with_history` already makes), not a static flag, so a
   shell that enters vim (alt screen) is handled correctly. A plugin may supply a
   hint/override, keeping the provider decoupled.

3. **In-memory ring**, appended in `reader_loop` (`backend/native.rs:189`) under
   the parser lock, right where `tx.send` happens — so it is ordered consistently
   with the broadcast and `seq`. `attach()` reads the ring under the same lock it
   already holds (`attach_with_history`, `backend/mod.rs:143`) and subscribes as
   today, so the live tail follows seamlessly.

4. **Client** (`Terminal.tsx:276`): the `term.clear()`-on-reconnect for the
   normal buffer stays — the raw replay repopulates cleanly, no double-append.
   The preamble must leave modes sane (normal buffer, reset attrs, home) so a
   ring that starts on an arbitrary chunk boundary self-heals within codex's next
   full repaint.

### Ring size

**Decision: ~8 MB per live session** (bytes). Codex's stream is redraw-heavy —
roughly 4 KB of raw bytes per *surviving* scrollback line — so 8 MB reaches the
start of typical sessions (the recorded ones were 3.8–7.3 MB) while bounding
worst-case memory. Note this is unrelated to `native.rs`'s `SCROLLBACK = 2000`
vt100 line cap, which is moot for codex (that scrollback is 0). History older
than the ring is not replayed; that gap is the domain of M4 cold-stitch (below).

## Relationship to other work

- **RF-VT100**: this is the concrete correctness defect behind that row's
  "unmaintained + deep-scrollback invariant" concern. TERM-SCROLL fixes the
  user-visible bug *without* replacing `vt100` (raw replay sidesteps the
  emulator). If RF-VT100 later swaps in `termwiz`/`alacritty_terminal` with
  correct region-scrollback, the alt-screen branch could fold back to a rendered
  repaint — but the ring is still the right consistency source for a live attach.
- **M4 cold-stitch** touches the same snapshot/attach surface and owns history
  *beyond* the in-memory ring (seed vt100 from a persisted snapshot, stitch cold
  history from SQLite). TERM-SCROLL's ring is the live/warm half; the two compose.
  Keep them independent per the "R/M-track must not assume each other" rule.

## As built (2026-07-08)

- `backend/mod.rs`: `HistoryRing` (bounded byte ring, evicts whole oldest chunks,
  never below one chunk so the current screen survives) + `HISTORY_RING_BYTES`
  (8 MB) + `raw_history_repaint` (preamble `ESC[?1049l ESC[r ESC[H ESC[2J ESC[3J
  ESC[m` + ring bytes). `attach_with_history` now takes the ring and branches on
  `screen().alternate_screen()`: alt-screen → `repaint_with_history` (unchanged,
  arms the alt buffer, subscribe under the parser lock); normal-buffer → raw ring
  replay, read + subscribe under the **ring** lock.
- `backend/native.rs` / `backend/sidecar.rs`: each session owns an
  `Arc<Mutex<HistoryRing>>`; `reader_loop` / `drain_loop` push the chunk and
  broadcast **inside the ring lock**, so a normal-buffer attach sees one
  consistent stream (ring end == stream start). `repaint_with_history` is now
  reached only for alt-screen; its `vt100`-scrollback history branch is retained
  (still unit-tested) but dead in production — a hook for RF-VT100.
- Client unchanged: the existing `term.clear()`-on-reconnect (`Terminal.tsx`) is
  covered by the preamble's screen+scrollback clear.

## Verification (run 2026-07-08)

- **Emulator matrix** (root cause + fix payload): the recorded codex streams and
  the controlled `ESC[1;4r` case through `vt100` (0 scrollback), xterm.js headless
  5.5 (907 / 1173), and tmux (~994) — the divergence. Replaying `preamble + ring`
  through xterm.js reconstructs the true first line (codex banner) continuous into
  the live viewport; a forced-truncated ring degrades to a clean cutoff (recent
  history + live screen correct, only blank lines at the very top).
- **End-to-end through the real daemon** — `scripts/termscroll-test.mjs`
  (self-contained, dependency-free): drives a shell session into the codex shape
  (bottom-margin region + 100 scrolled lines), attaches fresh, and asserts the
  snapshot begins with the raw-replay preamble and **contains `line-001`** — the
  oldest scrolled-off line, which the old rendered repaint could not carry. A
  second session in the alternate screen still gets the rendered `ESC[?1049h`
  repaint (claude path unchanged). ALL PASS.
- **Unit**: `backend::tests` — ring eviction, lone-oversized-chunk retention,
  normal-buffer raw replay reconstructs screen + scrollback (plain-scroll case,
  which `vt100` can reproduce in-crate), alt-screen stays rendered. 93 daemon
  tests pass; clippy clean.
