// Build and fire `vscode://` deep links that open a session's workspace in
// the VS Code installed on *this* machine (the browser's), never on the
// daemon host. A loopback daemon opens the folder directly; a remote daemon
// goes through Remote-SSH.

import { VscodeTarget } from "./api";
import { Target } from "./connectionStore";

export interface VscodeLaunch {
  uri: string;
  kind: "local" | "remote-ssh" | "unavailable";
  /** `user@host` VS Code will SSH to (remote-ssh only). */
  sshDest?: string;
  /** Terminal equivalent of the deep link (remote-ssh only). */
  cliCommand?: string;
}

const LOOPBACK = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]);

/**
 * Whether "Continue in VS Code" can work for a target at all.
 *
 * A `vscode://` deep link has to name a host the *client's* machine can reach
 * directly (a local folder, or an SSH destination). A relayed node is — by
 * design — only reachable through the relay; the client has no direct route to
 * it, and no `vscode://` scheme rides the relay. Firing the remote-ssh link
 * anyway would tell the user's VS Code to SSH into the *relay* machine (the
 * wrong host) at a path that only exists on the node. So we gate the feature
 * off for relayed targets. The planned fix is a browser-based editor served
 * from the node and proxied through the relay — see
 * docs/vscode-over-relay-plan.md.
 */
export function vscodeReachable(t: Target): boolean {
  return !t.relayKey;
}

export function buildVscodeLaunch(t: Target, info: VscodeTarget): VscodeLaunch {
  if (!vscodeReachable(t)) return { uri: "", kind: "unavailable" };

  // Normalize to a URI path: forward slashes, leading slash (covers Windows
  // drive paths), segments percent-encoded for spaces and friends.
  const slashed = info.path.replace(/\\/g, "/");
  const path = (slashed.startsWith("/") ? slashed : `/${slashed}`)
    .split("/")
    .map(encodeURIComponent)
    .join("/");

  if (!t.baseUrl) {
    // Same-origin ("This machine"): the daemon serving this page has the
    // browser's filesystem, so the folder path is directly openable. (Edge
    // case we accept: browsing an SSH-forwarded port directly instead of
    // adding it as a profile makes a remote daemon look same-origin.)
    return { uri: `vscode://file${path}`, kind: "local" };
  }

  // Every added profile is a remote daemon (direct LAN or an SSH-forwarded
  // loopback port — see connectionStore). A loopback URL host is the tunnel
  // case, where the forwarded port is useless as an SSH destination; fall
  // back to the hostname the daemon reports for itself.
  const urlHost = new URL(t.baseUrl).hostname;
  const host = LOOPBACK.has(urlHost) ? info.hostname : urlHost;
  const sshDest = info.ssh_user ? `${info.ssh_user}@${host}` : host;
  const quotedPath = /[^\w@%+=:,./-]/.test(info.path)
    ? `'${info.path.replace(/'/g, `'\\''`)}'`
    : info.path;
  return {
    uri: `vscode://vscode-remote/ssh-remote+${sshDest}${path}`,
    kind: "remote-ssh",
    sshDest,
    cliCommand: `code --remote ssh-remote+${sshDest} ${quotedPath}`,
  };
}

/**
 * Fire the deep link and report whether anything handled it. Browsers give no
 * direct signal, so use the meeting-link heuristic: if the page loses focus
 * or visibility shortly after navigating, an external app took over; if we
 * are still front-and-center when the timeout fires, VS Code almost certainly
 * is not installed and the caller should offer the download page.
 */
export function launchVscode(uri: string, timeoutMs = 2500): Promise<boolean> {
  return new Promise((resolve) => {
    let done = false;
    let timer: ReturnType<typeof setTimeout>;
    const finish = (opened: boolean) => {
      if (done) return;
      done = true;
      window.removeEventListener("blur", onAway);
      document.removeEventListener("visibilitychange", onVis);
      clearTimeout(timer);
      resolve(opened);
    };
    const onAway = () => finish(true);
    const onVis = () => {
      if (document.hidden) finish(true);
    };
    window.addEventListener("blur", onAway);
    document.addEventListener("visibilitychange", onVis);
    timer = setTimeout(() => finish(false), timeoutMs);
    // Navigating to a scheme nothing handles is a no-op, so this never
    // unloads the app.
    window.location.href = uri;
  });
}
