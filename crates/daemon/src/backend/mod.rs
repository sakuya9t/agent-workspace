use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use parking_lot::Mutex;
use tokio::sync::{broadcast, watch};

pub mod native;
pub mod sidecar;
pub mod asmux_client;

/// Per-session cap on the in-memory raw-output ring that backs the attach
/// snapshot for normal-buffer agents (codex, shell). Codex's stream is
/// redraw-heavy — roughly 4 KB of raw bytes per *surviving* scrollback line —
/// so 8 MB reaches the start of a typical session while bounding worst-case
/// memory. History older than this is not replayed on attach (that gap is M4
/// cold-stitch territory). See `docs/terminal-scrollback.md`.
pub(crate) const HISTORY_RING_BYTES: usize = 8 * 1024 * 1024;

/// A bounded ring of the raw PTY output chunks a session has produced, oldest
/// first. Fed to a fresh client on attach (for normal-buffer agents) so its own
/// terminal emulator reconstructs scrollback + screen exactly — the daemon's
/// `vt100` cannot, because it drops the scrollback of a bottom-margin scroll
/// region (the shape codex renders in). Whole chunks are evicted from the front
/// once the byte budget is exceeded, so replay may begin mid-frame; the repaint
/// preamble puts the client in a known state so the app's next full redraw
/// heals it.
pub(crate) struct HistoryRing {
    chunks: VecDeque<Arc<[u8]>>,
    bytes: usize,
    cap: usize,
}

impl HistoryRing {
    pub(crate) fn new(cap: usize) -> Self {
        Self { chunks: VecDeque::new(), bytes: 0, cap }
    }

    /// Append one raw output chunk, evicting whole oldest chunks to stay within
    /// the byte cap. MUST be called in the same critical section as the matching
    /// `tx.send`, so the ring and the broadcast stay a single consistent stream
    /// (see `attach_with_history`).
    pub(crate) fn push(&mut self, chunk: Arc<[u8]>) {
        self.bytes += chunk.len();
        self.chunks.push_back(chunk);
        while self.bytes > self.cap && self.chunks.len() > 1 {
            if let Some(front) = self.chunks.pop_front() {
                self.bytes -= front.len();
            }
        }
    }

    fn extend_into(&self, out: &mut Vec<u8>) {
        for c in &self.chunks {
            out.extend_from_slice(c);
        }
    }
}

/// One session the holder still knows about, from `holder_list()`. Used at
/// startup to decide adopt (alive) vs reconcile-from-exit (dead) vs
/// reconcile-indeterminate (absent — the holder itself was gone).
#[derive(Debug, Clone)]
pub struct HolderEntry {
    pub id: String,
    pub alive: bool,
    pub exit_code: i32,
    pub exit_signal: i32,
}

/// Everything a backend needs to spawn one live session.
#[derive(Debug, Clone)]
pub struct BackendSpawnSpec {
    pub session_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub rows: u16,
    pub cols: u16,
}

/// Live backend status, mirrored into the session record by the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendStatus {
    Running,
    Exited(i32),
    Failed(String),
}

impl BackendStatus {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, BackendStatus::Running)
    }
}

/// A server-side terminal snapshot: an ANSI repaint stream plus geometry.
/// Written through the same xterm.js path as live output on the client.
///
/// `rows`/`cols`/`last_seq` are part of the snapshot resume contract (they will
/// feed persisted-snapshot history and the client resume point); only
/// `repaint` is consumed today.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub rows: u16,
    pub cols: u16,
    pub repaint: Arc<[u8]>,
    pub last_seq: u64,
}

/// Render the emulator's scrollback (oldest first) as plain lines followed by
/// a full repaint of the visible screen. Fed to a fresh client terminal this
/// reproduces the screen AND fills the client's own scrollback, so the user
/// can scroll up to output from before they attached. Used for the attach
/// snapshot only — the mid-stream lag resend must stay screen-only, or every
/// resend would append the whole history to the client's scrollback again.
///
/// While the application holds the alternate screen, vt100 exposes a
/// zero-length scrollback and this degrades to the plain screen repaint
/// (matching real terminals, where the alternate screen has no scrollback).
///
/// Besides the contents, the repaint carries the terminal MODE state: which
/// buffer is active (DECSET 1049) and the input modes (mouse protocol,
/// bracketed paste, application cursor/keypad). TUIs like claude enable SGR
/// mouse reporting and scroll their transcript on wheel reports; a freshly
/// attached client that never saw those DECSETs treats the wheel as "scroll
/// local scrollback" — which is empty — and the wheel is dead until the app
/// happens to re-assert its modes on the next keystroke.
///
/// The parser's view offset is restored to 0 before returning.
pub(crate) fn repaint_with_history(parser: &mut vt100::Parser) -> Vec<u8> {
    let (rows, cols) = parser.screen().size();

    // View offsets are clamped to the available scrollback, so this measures it.
    parser.set_scrollback(usize::MAX);
    let available = parser.screen().scrollback();

    let mut out = Vec::new();
    // Sync the client's active buffer before anything else: the history below
    // must land in the normal buffer (the alternate one has no scrollback),
    // and an alt-screen app's repaint must land in the alternate buffer.
    out.extend_from_slice(if parser.screen().alternate_screen() {
        b"\x1b[?1049h"
    } else {
        b"\x1b[?1049l"
    });
    if available > 0 {
        // Home + erase: the history lines below carry no positioning of their
        // own, and a reconnecting client may have its cursor anywhere.
        out.extend_from_slice(b"\x1b[H\x1b[J");
    }
    // At view offset `k`, the window's first visible row is the
    // (available-k)'th-oldest scrollback line — walking the offset down to 1
    // emits every scrollback line exactly once, oldest first.
    //
    // INVARIANT: only the FIRST visible row may be read at offsets deeper than
    // the screen height. vt100 0.15's `visible_rows` miscomputes (and, with
    // overflow checks, panics on) the screen-row tail of the window for
    // `offset > rows`; the leading scrollback rows are correct at any depth.
    // Overflow checks are disabled for vt100 in Cargo.toml for this reason.
    for offset in (1..=available).rev() {
        parser.set_scrollback(offset);
        if let Some(line) = parser.screen().rows_formatted(0, cols).next() {
            out.extend_from_slice(&line);
        }
        out.extend_from_slice(b"\x1b[m\r\n");
    }
    parser.set_scrollback(0);
    if available > 0 {
        // Scroll the emitted lines fully into the client's scrollback: the
        // repaint below starts by erasing the viewport (\x1b[H\x1b[J), and any
        // history line still visible would be erased rather than scrolled back.
        // After the history print the cursor row is min(available, rows-1), so
        // rows-1 newlines push out exactly the visible history lines without
        // ever pushing the blank cursor row (a spurious empty history line).
        for _ in 0..rows.saturating_sub(1) {
            out.extend_from_slice(b"\r\n");
        }
    }
    out.extend_from_slice(&parser.screen().contents_formatted());
    out.extend_from_slice(&parser.screen().input_mode_formatted());
    out
}

/// Preamble + raw ring, the attach repaint for a **normal-buffer** session.
/// The preamble parks the client in a known state — normal buffer, no scroll
/// region, cleared screen + scrollback, default attributes — so a ring that was
/// truncated to its byte cap (and may therefore begin mid-frame), or a client
/// reconnecting with a stale scroll region, both render cleanly once the app's
/// next full repaint lands. The ring bytes then reconstruct scrollback AND the
/// current screen in the client's own emulator (it ends at the live screen, so
/// no separate screen repaint is appended).
fn raw_history_repaint(history: &HistoryRing) -> Vec<u8> {
    let mut out = Vec::with_capacity(history.bytes + 16);
    out.extend_from_slice(b"\x1b[?1049l\x1b[r\x1b[H\x1b[2J\x1b[3J\x1b[m");
    history.extend_into(&mut out);
    out
}

/// Shared `BackendSession::attach` body for the emulator-holding backends.
/// Captures the attach repaint and subscribes to the output stream so the
/// receiver's first byte is exactly the one after the repaint — no gap, no
/// duplicate. Two paths, by how the agent renders:
///
/// - **Alternate screen** (claude, or any TUI holding `?1049`): the app owns its
///   own scrolling, so the repaint is `repaint_with_history` (arms the alt
///   buffer + mouse/paste modes + screen, no history). Subscribe under the
///   parser lock — the writer processes under that lock, so this stays ordered.
/// - **Normal buffer** (codex, shell): the daemon's `vt100` drops the scrollback
///   of the bottom-margin scroll region codex renders in, so a rendered repaint
///   would carry no history. Replay the raw byte ring instead; read it and
///   subscribe under the *ring* lock — the writer pushes+broadcasts under that
///   same lock, so the live stream begins precisely where the ring ends.
///
/// See `docs/terminal-scrollback.md`.
pub(crate) fn attach_with_history(
    parser: &Mutex<vt100::Parser>,
    history: &Mutex<HistoryRing>,
    tx: &broadcast::Sender<Arc<[u8]>>,
    seq: &AtomicU64,
) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>) {
    let mut parser = parser.lock();
    let (rows, cols) = parser.screen().size();

    if parser.screen().alternate_screen() {
        let repaint: Arc<[u8]> =
            Arc::from(repaint_with_history(&mut parser).into_boxed_slice());
        let rx = tx.subscribe();
        let last_seq = seq.load(Ordering::SeqCst);
        drop(parser);
        return (Snapshot { rows, cols, repaint, last_seq }, rx);
    }

    let history = history.lock();
    let repaint: Arc<[u8]> = Arc::from(raw_history_repaint(&history).into_boxed_slice());
    let rx = tx.subscribe();
    let last_seq = seq.load(Ordering::SeqCst);
    drop(history);
    drop(parser);
    (Snapshot { rows, cols, repaint, last_seq }, rx)
}

/// Shared `BackendSession::snapshot` body: a screen-only repaint (no history
/// replay) of the current emulator state.
pub(crate) fn snapshot_screen(parser: &vt100::Parser, seq: &AtomicU64) -> Snapshot {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let repaint: Arc<[u8]> = Arc::from(screen.contents_formatted().into_boxed_slice());
    Snapshot {
        rows,
        cols,
        repaint,
        last_seq: seq.load(Ordering::SeqCst),
    }
}

/// Factory for live sessions. The native backend is registered under the
/// plugin registry; a mock backend implements the same trait in tests.
pub trait SessionBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn create(&self, spec: BackendSpawnSpec) -> Result<Arc<dyn BackendSession>>;

    /// Whether live sessions survive a daemon shutdown. `true` for an
    /// out-of-process holder (asmux): on shutdown the daemon detaches and leaves
    /// the children running to be re-adopted, rather than killing them. `false`
    /// for the in-process native backend, whose PTYs must be reaped.
    fn keep_sessions_on_shutdown(&self) -> bool {
        false
    }

    /// Sessions the out-of-process holder still knows about (empty for backends
    /// that don't outlive the daemon).
    fn holder_list(&self) -> Result<Vec<HolderEntry>> {
        Ok(Vec::new())
    }

    /// Re-adopt a session that is still alive in the holder after a daemon
    /// restart: seed a fresh daemon-side emulator from cold history and re-attach
    /// the holder ring. Returns `None` if this backend cannot adopt (native) or
    /// the session is not recoverable.
    fn adopt(&self, _session_id: &str, _rows: u16, _cols: u16) -> Result<Option<Arc<dyn BackendSession>>> {
        Ok(None)
    }
}

/// A single live session owned by a backend.
pub trait BackendSession: Send + Sync {
    /// Atomically capture the current emulator snapshot and subscribe to the
    /// live output stream. Ordering between snapshot and stream is guaranteed
    /// so a fresh client never sees a gap or a duplicated byte range.
    fn attach(&self) -> (Snapshot, broadcast::Receiver<Arc<[u8]>>);

    /// Current emulator snapshot without subscribing.
    fn snapshot(&self) -> Snapshot;

    /// Plain-text rendering of the current visible screen — rows joined by `\n`,
    /// no formatting escapes. Screen-based attention classifiers read this
    /// instead of the raw output byte stream, so a prompt whose question isn't
    /// the last thing written (a boxed menu, a redraw-frame tail) is still seen.
    fn screen_text(&self) -> String;

    fn send_input(&self, data: &[u8]) -> Result<()>;
    fn resize(&self, rows: u16, cols: u16) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn watch_status(&self) -> watch::Receiver<BackendStatus>;
    fn last_seq(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::{
        attach_with_history, repaint_with_history, HistoryRing, HISTORY_RING_BYTES,
    };
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    /// The normal-buffer preamble every raw-replay repaint starts with: normal
    /// buffer, reset scroll region, clear screen + scrollback, reset attrs.
    const PREAMBLE: &[u8] = b"\x1b[?1049l\x1b[r\x1b[H\x1b[2J\x1b[3J\x1b[m";

    /// Feed the attach repaint to a fresh client-side emulator (standing in
    /// for xterm.js) and check both the visible screen and the scrollback.
    #[test]
    fn attach_repaint_reproduces_screen_and_history() {
        let mut server = vt100::Parser::new(5, 20, 100);
        for i in 1..=12 {
            server.process(format!("line {i}\r\n").as_bytes());
        }
        server.process(b"\x1b[31mprompt>\x1b[m ");

        let repaint = repaint_with_history(&mut server);
        assert_eq!(server.screen().scrollback(), 0, "view offset restored");
        // Buffer sync first (a reconnecting client may sit in the alternate
        // buffer, where history lines would not reach the scrollback), then
        // home+erase — the client's cursor can be anywhere.
        assert!(repaint.starts_with(b"\x1b[?1049l\x1b[H\x1b[J"));

        let mut client = vt100::Parser::new(5, 20, 1000);
        client.process(&repaint);

        // The visible screen and cursor match the server's exactly.
        assert_eq!(client.screen().contents(), server.screen().contents());
        assert_eq!(
            client.screen().cursor_position(),
            server.screen().cursor_position()
        );

        // 13 lines painted on a 5-row screen leave exactly 8 in scrollback —
        // no gap and no spurious blank line between history and screen.
        client.set_scrollback(usize::MAX);
        assert_eq!(client.screen().scrollback(), 8);
        let oldest = client.screen().contents();
        assert!(oldest.starts_with("line 1\n"), "oldest window: {oldest:?}");
    }

    #[test]
    fn attach_repaint_without_scrollback_is_screen_only() {
        let mut server = vt100::Parser::new(5, 20, 100);
        server.process(b"hello");
        let mut expected = b"\x1b[?1049l".to_vec();
        expected.extend_from_slice(&server.screen().contents_formatted());
        expected.extend_from_slice(&server.screen().input_mode_formatted());
        assert_eq!(repaint_with_history(&mut server), expected);
    }

    /// TUI apps own the alternate screen; there the snapshot must stay a plain
    /// screen repaint (real terminals have no alt-screen scrollback either) —
    /// no history replay, no home+erase preamble.
    #[test]
    fn attach_repaint_in_alternate_screen_is_screen_only() {
        let mut server = vt100::Parser::new(5, 20, 100);
        for i in 1..=12 {
            server.process(format!("line {i}\r\n").as_bytes());
        }
        server.process(b"\x1b[?1049h\x1b[Hfullscreen app");
        let mut expected = b"\x1b[?1049h".to_vec();
        expected.extend_from_slice(&server.screen().contents_formatted());
        expected.extend_from_slice(&server.screen().input_mode_formatted());
        assert_eq!(repaint_with_history(&mut server), expected);
        assert!(server.screen().alternate_screen());
    }

    /// Attach while a TUI owns the alternate screen with mouse reporting on
    /// (claude, codex): the repaint must arm the client's alternate buffer,
    /// mouse protocol, and bracketed paste, or wheel events over the client
    /// terminal try to scroll its (empty) local scrollback and do nothing
    /// until the app happens to re-assert its modes.
    #[test]
    fn attach_repaint_replays_alt_screen_and_input_modes() {
        let mut server = vt100::Parser::new(5, 20, 100);
        server.process(b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[Hfullscreen app");

        let mut client = vt100::Parser::new(5, 20, 1000);
        client.process(&repaint_with_history(&mut server));

        assert!(client.screen().alternate_screen());
        assert_eq!(
            client.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::ButtonMotion
        );
        assert_eq!(
            client.screen().mouse_protocol_encoding(),
            vt100::MouseProtocolEncoding::Sgr
        );
        assert!(client.screen().bracketed_paste());
        assert_eq!(client.screen().contents(), server.screen().contents());
    }

    // ---- TERM-SCROLL: raw-history ring + per-buffer-model attach ----

    /// The ring stays under its byte cap by evicting whole oldest chunks, never
    /// dropping below one chunk (so the newest write always survives).
    #[test]
    fn history_ring_evicts_oldest_over_cap() {
        let mut ring = HistoryRing::new(10);
        for i in 0..6u8 {
            ring.push(Arc::from(vec![i, i, i, i].into_boxed_slice())); // 4-byte chunks
        }
        // cap=10, 4-byte chunks: at most 2 fit (8 ≤ 10); a 3rd would push to 12.
        assert!(ring.bytes <= 10, "bytes over cap: {}", ring.bytes);
        let mut out = Vec::new();
        ring.extend_into(&mut out);
        // Oldest chunks evicted; the two most recent (4,4,4,4 / 5,5,5,5) remain.
        assert_eq!(out, vec![4, 4, 4, 4, 5, 5, 5, 5]);
    }

    /// A single chunk larger than the cap is still retained (eviction never
    /// empties the ring), so the current screen is never lost.
    #[test]
    fn history_ring_keeps_a_lone_oversized_chunk() {
        let mut ring = HistoryRing::new(4);
        ring.push(Arc::from(vec![1, 2, 3, 4, 5, 6, 7, 8].into_boxed_slice()));
        let mut out = Vec::new();
        ring.extend_into(&mut out);
        assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    fn attach_fixture(
        raw: &[u8],
    ) -> (Mutex<vt100::Parser>, Mutex<HistoryRing>, broadcast::Sender<Arc<[u8]>>, AtomicU64) {
        let mut parser = vt100::Parser::new(5, 20, 1000);
        parser.process(raw);
        let mut ring = HistoryRing::new(HISTORY_RING_BYTES);
        ring.push(Arc::from(raw.to_vec().into_boxed_slice()));
        let (tx, _keep) = broadcast::channel(16);
        (Mutex::new(parser), Mutex::new(ring), tx, AtomicU64::new(0))
    }

    /// Normal-buffer attach replays the raw ring (preamble + bytes), and feeding
    /// it to a fresh emulator reconstructs the screen AND the scrollback —
    /// matching a client that had processed the raw stream live. (Uses a plain
    /// full-screen scroll, which vt100 *does* capture, so the reconstruction is
    /// checkable in-crate; the bottom-margin-region case only xterm.js/tmux
    /// reconstruct and is covered by the headless harness.)
    #[test]
    fn attach_normal_buffer_replays_raw_ring() {
        let mut raw = Vec::new();
        for i in 1..=12 {
            raw.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        raw.extend_from_slice(b"\x1b[31mprompt>\x1b[m ");

        let (parser, history, tx, seq) = attach_fixture(&raw);
        let (snap, _rx) = attach_with_history(&parser, &history, &tx, &seq);

        assert!(snap.repaint.starts_with(PREAMBLE), "missing normal-buffer preamble");
        assert!(
            snap.repaint.windows(raw.len()).any(|w| w == raw.as_slice()),
            "repaint does not contain the raw ring bytes"
        );

        // Reference: a client that saw the raw stream live.
        let mut reference = vt100::Parser::new(5, 20, 1000);
        reference.process(&raw);
        // Fresh client fed only the attach repaint.
        let mut client = vt100::Parser::new(5, 20, 1000);
        client.process(&snap.repaint);

        assert_eq!(client.screen().contents(), reference.screen().contents());
        assert_eq!(
            client.screen().cursor_position(),
            reference.screen().cursor_position()
        );
        client.set_scrollback(usize::MAX);
        reference.set_scrollback(usize::MAX);
        assert_eq!(client.screen().scrollback(), reference.screen().scrollback());
        assert!(client.screen().scrollback() > 0, "expected reconstructed scrollback");
    }

    /// Alternate-screen attach (claude) is unchanged: a rendered repaint that
    /// arms the alt buffer — never the normal-buffer raw-replay preamble.
    #[test]
    fn attach_alt_screen_stays_rendered_repaint() {
        let (parser, history, tx, seq) =
            attach_fixture(b"\x1b[?1049h\x1b[?1006h\x1b[Hfullscreen app");
        let (snap, _rx) = attach_with_history(&parser, &history, &tx, &seq);

        assert!(snap.repaint.starts_with(b"\x1b[?1049h"), "alt repaint must arm alt buffer");
        assert!(!snap.repaint.starts_with(PREAMBLE), "alt path must not raw-replay");

        let mut client = vt100::Parser::new(5, 20, 1000);
        client.process(&snap.repaint);
        assert!(client.screen().alternate_screen());
    }
}
