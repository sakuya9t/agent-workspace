import type { Logger, LogErrorOptions, ProxyOptions } from "vite";

// Connection-level failures that mean "the daemon isn't answering", as they
// appear inside node-http-proxy's error messages/stacks.
const DAEMON_DOWN = /ECONNREFUSED|ECONNRESET|ETIMEDOUT|EHOSTUNREACH|EPIPE|socket hang up/;

/** True for the ECONNREFUSED-style proxy errors Vite logs when the daemon is down. */
export function isDaemonDownProxyError(msg: unknown): boolean {
  return typeof msg === "string" && /proxy (socket )?error/.test(msg) && DAEMON_DOWN.test(msg);
}

/** JSON body the browser receives when the dev proxy can't reach the daemon. */
export function daemonDownBody(daemon: string): string {
  return JSON.stringify({
    error: `cannot connect — daemon not running at ${daemon}? Start it with \`cargo run -p asm-daemon\`.`,
  });
}

/**
 * Proxy `configure` hook: answer connection failures with 502 + a JSON error
 * the client shows verbatim ("cannot connect — …") instead of Vite's default
 * bodyless 500, which surfaced in the UI as "500 Internal Server Error".
 * Listeners added in `configure` run before Vite's own error handler, which
 * then skips its 500 because headers are already sent. WebSocket upgrade
 * errors hand us a raw socket (no `writeHead`) and are left to Vite.
 */
export function respondDaemonDown(daemon: string): NonNullable<ProxyOptions["configure"]> {
  return (proxy) => {
    proxy.on("error", (_err, _req, res) => {
      if (!res || !("writeHead" in res) || res.headersSent || res.writableEnded) return;
      res.writeHead(502, { "content-type": "application/json" }).end(daemonDownBody(daemon));
    });
  };
}

/**
 * Wrap a Vite logger so daemon-down proxy errors collapse into one throttled,
 * actionable hint instead of a stack trace per polled request. All other logs
 * pass through untouched. `now` is injectable for testing.
 */
export function createDaemonAwareLogger(
  base: Logger,
  daemon: string,
  now: () => number = Date.now,
): Logger {
  let lastWarn = -Infinity; // ensure the first hint always fires, whatever the clock reads
  return {
    ...base,
    error(msg: string, opts?: LogErrorOptions) {
      if (isDaemonDownProxyError(msg)) {
        const t = now();
        if (t - lastWarn > 3000) {
          lastWarn = t;
          base.warn(
            `[vite] daemon not reachable at ${daemon} — start it with \`cargo run -p asm-daemon\` (or set ASM_DAEMON)`,
          );
        }
        return;
      }
      base.error(msg, opts);
    },
  };
}
