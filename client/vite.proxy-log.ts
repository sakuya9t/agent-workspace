import type { Logger, LogErrorOptions } from "vite";

// Connection-level failures that mean "the daemon isn't answering", as they
// appear inside node-http-proxy's error messages/stacks.
const DAEMON_DOWN = /ECONNREFUSED|ECONNRESET|ETIMEDOUT|EHOSTUNREACH|EPIPE|socket hang up/;

/** True for the ECONNREFUSED-style proxy errors Vite logs when the daemon is down. */
export function isDaemonDownProxyError(msg: unknown): boolean {
  return typeof msg === "string" && /proxy (socket )?error/.test(msg) && DAEMON_DOWN.test(msg);
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
