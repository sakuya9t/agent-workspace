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
