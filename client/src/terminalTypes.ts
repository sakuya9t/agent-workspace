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
}

/**
 * Mobile Ctrl key latch: `armed` transforms the next typed key into its control
 * code once; `locked` keeps transforming until toggled off (double-tap).
 */
export type CtrlLatch = "off" | "armed" | "locked";
