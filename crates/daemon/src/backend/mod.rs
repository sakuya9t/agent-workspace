use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, watch};

pub mod native;
pub mod sidecar;
pub mod asmux_client;

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

    fn send_input(&self, data: &[u8]) -> Result<()>;
    fn resize(&self, rows: u16, cols: u16) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn watch_status(&self) -> watch::Receiver<BackendStatus>;
    fn last_seq(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::repaint_with_history;

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
}
