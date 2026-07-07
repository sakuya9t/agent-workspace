/**
 * Copy text to the OS clipboard, tolerating insecure contexts.
 *
 * `navigator.clipboard` requires a secure context (HTTPS or localhost); plain-
 * http LAN daemon profiles don't have one, so fall back to the legacy
 * `execCommand("copy")` path via an off-screen textarea. Must be called from
 * within a user gesture (keydown / click / contextmenu) for the fallback to be
 * permitted by the browser.
 */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    /* fall through to the selection-based path */
  }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  ta.remove();
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
