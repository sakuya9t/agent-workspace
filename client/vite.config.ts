import { defineConfig, type ProxyOptions } from "vite";
import react from "@vitejs/plugin-react";
import type { IncomingMessage, ServerResponse } from "node:http";
import type { Socket } from "node:net";

// Proxy API + WebSocket traffic to the local daemon during development.
const daemon = process.env.ASM_DAEMON ?? "http://127.0.0.1:4600";

// When the daemon isn't up, node-http-proxy emits an ECONNREFUSED that Vite
// prints as a raw stack trace on every request (and /health is polled), while
// the browser request hangs. Swallow it into one throttled, actionable line
// and return a clean 502 so the client sees a real error instead of a hang.
let lastWarn = 0;
function warnDaemonDown(err: NodeJS.ErrnoException) {
  const now = Date.now();
  if (now - lastWarn < 3000) return;
  lastWarn = now;
  const why = err.code === "ECONNREFUSED" ? "not reachable" : err.code ?? err.message;
  console.warn(
    `[vite] daemon ${why} at ${daemon} — start it with \`cargo run -p asm-daemon\` (or set ASM_DAEMON)`,
  );
}

const proxyEntry = (ws = false): ProxyOptions => ({
  target: daemon,
  changeOrigin: true,
  ws,
  configure: (proxy) => {
    proxy.on(
      "error",
      (err: NodeJS.ErrnoException, _req: IncomingMessage, res: ServerResponse | Socket) => {
        warnDaemonDown(err);
        // WebSocket upgrades hand us a raw Socket; just close it.
        if (!("writeHead" in res)) {
          res.destroy();
          return;
        }
        if (res.headersSent || res.writableEnded) return;
        res.writeHead(502, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "daemon_unreachable", target: daemon }));
      },
    );
  },
});

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5273,
    proxy: {
      "/api": proxyEntry(true),
      "/health": proxyEntry(),
    },
  },
});
