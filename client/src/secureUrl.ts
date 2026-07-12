/**
 * Transport check for a URL the user types into the connection dialog.
 *
 * Everything a client sends to a daemon — the device token, every keystroke,
 * every byte of terminal output — rides this URL, so a plaintext one to a
 * remote host puts all of it on the wire in the clear. The dialog is where
 * those URLs enter the app, so it is where this is caught.
 *
 * Loopback is deliberately exempt: `http://localhost:4600` is the local daemon
 * and the far end of an SSH port-forward, both of which are already encrypted
 * (or never left the machine). Browsers agree — they treat loopback as a secure
 * context. Demanding https there would only buy a self-signed certificate and a
 * click-through warning.
 */
export type UrlProblem = "invalid" | "insecure" | "websocket";

export function checkTargetUrl(raw: string): UrlProblem | null {
  let u: URL;
  try {
    u = new URL(raw);
  } catch {
    return "invalid";
  }
  // A `wss://` URL is a plausible thing to paste — it is what the *daemon* uses
  // to register with a relay. But everything the browser does with this URL goes
  // through `fetch` (enrollment, /health, relay node discovery), which only
  // speaks http/https; the WebSocket URL is derived from it later, in
  // `streamUrl`. Accepting `wss://` here would save a connection that could
  // never load anything.
  if (u.protocol === "ws:" || u.protocol === "wss:") return "websocket";
  if (u.protocol !== "http:" && u.protocol !== "https:") return "invalid";
  if (u.protocol === "https:") return null;
  return isLoopbackHost(u.hostname) ? null : "insecure";
}

function isLoopbackHost(hostname: string): boolean {
  // URL.hostname keeps the brackets on an IPv6 literal.
  const host = hostname.replace(/^\[|\]$/g, "").toLowerCase();
  return (
    host === "localhost" ||
    host.endsWith(".localhost") ||
    host === "::1" ||
    // The whole 127.0.0.0/8 block, not just 127.0.0.1.
    /^127\.\d+\.\d+\.\d+$/.test(host)
  );
}
