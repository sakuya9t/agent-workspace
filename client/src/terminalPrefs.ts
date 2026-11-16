import { create } from "zustand";

/**
 * Per-browser terminal preferences (docs/terminal-history-limit.md).
 *
 * The history limit bounds how much text each terminal window holds on THIS
 * device, so a very long session cannot slow the page down or crash the tab.
 * It is per browser rather than per host because what a page can comfortably
 * hold is a property of the device it runs on: a phone wants less than a
 * desktop looking at the same session. It never affects what is recorded or
 * exported — "Save conversation" reads the full record on the daemon.
 */

/** Choices offered in Settings, in characters. */
export const HISTORY_LIMIT_CHOICES = [250_000, 500_000, 1_000_000, 2_000_000, 5_000_000] as const;
export const DEFAULT_HISTORY_CHARS = 1_000_000;

/**
 * Raw bytes of replay allowed per character of the limit. A terminal stream is
 * far more than its visible text — escape codes, colors, and TUI redraws (codex
 * spends ~4 KB of stream per line that survives into scrollback) — so the
 * replay a fresh attach takes in is budgeted separately from what stays on
 * screen. At 8, the default 1M-character limit asks for 8 MB: exactly the
 * daemon's whole replay ring, so the default changes nothing about attaches.
 * The daemon clamps the request to its own range.
 */
const REPLAY_BYTES_PER_CHAR = 8;

/** xterm's scrollback, in lines, stays inside this range whatever the width. */
const MIN_SCROLLBACK_LINES = 200;
const MAX_SCROLLBACK_LINES = 50_000;

const KEY = "asm.terminalHistoryChars";

function load(): number {
  try {
    const n = Number(localStorage.getItem(KEY));
    if ((HISTORY_LIMIT_CHOICES as readonly number[]).includes(n)) return n;
  } catch {
    /* storage unavailable — fall through to the default */
  }
  return DEFAULT_HISTORY_CHARS;
}

interface TerminalPrefs {
  historyChars: number;
  setHistoryChars: (n: number) => void;
}

export const useTerminalPrefs = create<TerminalPrefs>((set) => ({
  historyChars: load(),
  setHistoryChars: (n) => {
    set({ historyChars: n });
    try {
      localStorage.setItem(KEY, String(n));
    } catch {
      /* the choice still applies to this page */
    }
  },
}));

/**
 * The limit as xterm scrollback lines at the terminal's current width. xterm
 * stores a fixed number of cells per line, so characters ÷ columns is what
 * bounds both the text kept and the memory behind it.
 */
export function scrollbackLines(historyChars: number, cols: number): number {
  const lines = Math.ceil(historyChars / Math.max(1, cols));
  return Math.min(MAX_SCROLLBACK_LINES, Math.max(MIN_SCROLLBACK_LINES, lines));
}

/** The replay budget (bytes) a terminal asks the daemon for on attach. */
export function replayBytes(historyChars: number): number {
  return historyChars * REPLAY_BYTES_PER_CHAR;
}
