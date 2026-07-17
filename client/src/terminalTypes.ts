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
 * Type a prompt into the live TUI and submit it, the way a user would.
 *
 * The Enter is a *separate* write, a beat after the text — never `text + "\r"`
 * in one go. Real typing reaches the pty as many discrete writes with Enter as
 * its own final event; a lone `write("…\r")` arrives as one pty read, and agent
 * TUIs (Claude Code among them) read a byte-burst that ends in a newline as a
 * paste, keeping the newline literal instead of submitting. That's why the text
 * lands in the prompt but only "sometimes" fires: submitting hinges on whether
 * the OS happened to split the `\r` into its own read. Sending Enter on its own,
 * after the burst window, makes it an unambiguous keypress every time.
 */
export function submitPrompt(handle: TerminalHandle | null | undefined, text: string) {
  if (!handle) return;
  handle.write(text);
  // Long enough to clear the TUI's paste/burst-coalescing window and a render
  // frame, short enough to still read as instant. Reuse `handle` (not a fresh
  // ref read) so the Enter targets the same terminal the text went to; if that
  // terminal is gone, its write is a safe no-op (the WS is closed).
  setTimeout(() => handle.write("\r"), 100);
}
