/**
 * Copy text to the OS clipboard, tolerating insecure contexts. Resolves true
 * only when the text actually reached the clipboard, so callers can show a
 * receipt instead of failing silently.
 *
 * `navigator.clipboard` requires a secure context (HTTPS or localhost); plain-
 * http LAN/relay profiles don't have one, so fall back to the legacy
 * `execCommand("copy")` path via an off-screen textarea. Must be called from
 * within a user gesture (keydown / click / contextmenu) for the fallback to be
 * permitted by the browser.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    /* fall through to the selection-based path */
  }
  // The textarea select() steals focus. Restore it afterwards: this runs with
  // focus in xterm's hidden textarea, and a terminal that silently loses focus
  // stops taking keys — including the paste the user reaches for next.
  const prev = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch {
    /* execCommand may throw instead of returning false */
  }
  ta.remove();
  prev?.focus();
  return ok;
}

/**
 * Whether the clipboard can be READ at all in this context. Reading has no
 * execCommand fallback — `document.execCommand("paste")` is denied to web content
 * in every browser — so `navigator.clipboard.readText` is the only path, and it
 * exists only in a secure context (HTTPS or localhost).
 *
 * The daemon and the relay both serve plain HTTP today (relay TLS is still open —
 * see docs/security-followups.md), so on a phone this is FALSE: the page is not a
 * secure context and `navigator.clipboard` is undefined. Copy is unaffected — it
 * falls back to execCommand — which is exactly why a broken Paste sat next to a
 * working Copy. Callers must offer a paste path that needs no clipboard read (see
 * PasteSheet), and must check this SYNCHRONOUSLY inside the click handler: on iOS
 * a textarea only raises the keyboard while the user gesture is still live.
 */
export function canReadClipboard(): boolean {
  return window.isSecureContext && typeof navigator.clipboard?.readText === "function";
}

/**
 * Read text from the OS clipboard, or "" when unavailable — including when the
 * user dismisses the confirmation Safari raises for every clipboard read. Used by
 * the mobile key bar's Paste, since long-press paste into xterm is unreliable on
 * touch. Guard it with canReadClipboard() and have a fallback for "".
 */
export async function readText(): Promise<string> {
  try {
    return await navigator.clipboard.readText();
  } catch {
    return "";
  }
}
