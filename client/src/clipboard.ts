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
 * Read text from the OS clipboard, or "" when unavailable. Unlike copy, reading
 * has no execCommand fallback — `navigator.clipboard.readText` needs a secure
 * context and user permission (the relay path is HTTPS; plain-http LAN profiles
 * simply return ""). Used by the mobile key bar's Paste, since long-press paste
 * into xterm is unreliable on touch.
 */
export async function readText(): Promise<string> {
  try {
    return await navigator.clipboard.readText();
  } catch {
    return "";
  }
}
