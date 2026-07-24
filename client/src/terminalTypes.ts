/** Shared types for the terminal input handle + mobile Ctrl latch. Kept in a
 *  standalone module so TerminalView and TermKeyBar can both import them without
 *  a component↔component cycle. */

/** Imperative handle a shell can grab off a mounted TerminalView. */
export interface TerminalHandle {
  /** Send raw bytes over the same WS input path as typed keys. */
  write: (data: string) => void;
  /** Focus xterm — summons the soft keyboard on iOS (call from a gesture). */
  focus: () => void;
  /** Current selection text (for the key bar's Copy). */
  getSelection: () => string;
  /** Return the view to the live tail — the terminal's own scrollback, and any
   *  scroll the running app is holding (see TerminalView's scroll-state block). */
  scrollToEnd: () => void;
}

/**
 * Mobile Ctrl key latch: `armed` transforms the next typed key into its control
 * code once; `locked` keeps transforming until toggled off (double-tap).
 */
export type CtrlLatch = "off" | "armed" | "locked";

/**
 * Soft return — a newline inside the agent's composer instead of a send.
 *
 * Plain Enter has to keep submitting, so the soft return must arrive as some
 * OTHER byte sequence that every agent reads as "insert a newline". ESC+CR
 * ("Meta+Enter") is that sequence, and it is Claude Code's own choice:
 * `/terminal-setup` writes exactly `{"key": "shift+enter", "command":
 * "workbench.action.terminal.sendSequence", "args": {"text": "\x1B\r"}}` into
 * VS Code's keybindings. Verified by hand against every agent we drive — claude
 * 2.1.219, codex 0.144.6, opencode 1.17.18 — all three insert a newline.
 *
 * A bare LF (`\n`, i.e. Ctrl+J) works in those three as well, and is the wrong
 * choice anyway: a `shell` session is also a session here, and to readline LF
 * *is* accept-line — Shift+Enter would silently RUN the half-typed command.
 * ESC+CR is unbound there (readline just beeps), so on the one target that has
 * no composer to insert into, the keystroke is inert rather than destructive.
 */
export const SOFT_RETURN = "\x1b\r";

/**
 * How long to wait after the text before sending the Enter.
 *
 * This has to clear the *Enter-suppression window* of the slowest agent TUI we
 * drive, which is a longer bar than merely landing in a separate pty read.
 * Codex names its window `PASTE_ENTER_SUPPRESS_WINDOW` — 120ms as of 0.144 — and
 * Claude Code suppresses an Enter that arrives right behind a paste too. 250ms
 * clears both with room to spare and still reads as instant.
 */
const ENTER_GAP_MS = 250;

/**
 * Type a prompt into the live TUI and submit it, the way a user would.
 *
 * The Enter is a *separate* write, a beat after the text — never `text + "\r"`
 * in one go. Agent TUIs coalesce a fast byte burst into a paste, and then
 * deliberately keep a newline literal for a moment afterwards, so that pasting
 * multi-line text can't submit itself halfway through. An Enter landing inside
 * that window is inserted into the composer instead of sending it: the prompt
 * sits in the input box with the cursor dropped to a fresh line.
 *
 * The gap therefore has to outlast that window, not just separate the writes.
 * The old 100ms sat *inside* Codex's 120ms one, and only submitted when Codex's
 * own ~8ms flush happened to retire the window first — a race a busy TUI loses.
 * That's why the button worked against Claude Code but left Codex holding an
 * unsent prompt.
 */
export function submitPrompt(handle: TerminalHandle | null | undefined, text: string) {
  if (!handle) return;
  handle.write(text);
  // Reuse `handle` (not a fresh ref read) so the Enter targets the same terminal
  // the text went to; if that terminal is gone, its write is a safe no-op (the
  // WS is closed).
  setTimeout(() => handle.write("\r"), ENTER_GAP_MS);
}
