import { defineConfig, createLogger } from "vite";
import react from "@vitejs/plugin-react";
import { createDaemonAwareLogger, respondDaemonDown } from "./vite.proxy-log";

// Proxy API + WebSocket traffic to the local daemon during development.
const daemon = process.env.ASM_DAEMON ?? "http://127.0.0.1:4600";
// UI-only gateways may proxy to an off-host daemon, where loopback trust does
// not apply. Inject an enrolled device token server-side for both HTTP and the
// WebSocket upgrade, keeping it out of browser storage and URLs.
const daemonToken = process.env.ASM_DAEMON_TOKEN;
const daemonAuth = daemonToken
  ? { headers: { Authorization: `Bearer ${daemonToken}` } }
  : {};

// Host the dev server binds to. Default `true` = all interfaces (0.0.0.0 + ::),
// so the React client is reachable from other machines on the LAN — point a
// browser at http://<this-host>:5273. This mirrors binding the daemon to
// 0.0.0.0 (see scripts/wizard.sh). Set ASM_CLIENT_HOST=127.0.0.1 to restrict
// back to loopback only.
// NOTE: the /api + /health + ws proxy dials the daemon from 127.0.0.1, so LAN
// clients reach it over the daemon's loopback-trusted path (no token). Only
// bind to the network on a trusted LAN.
const host =
  process.env.ASM_CLIENT_HOST === undefined ? true : process.env.ASM_CLIENT_HOST;

// When the daemon isn't up, node-http-proxy raises a connection error on every
// request (and /health is polled), which Vite would log as a full stack trace
// on each one — flooding the console. Collapse that specific noise into one
// throttled, actionable hint; everything else logs normally. Vite's own proxy
// handler still ends the response, so requests fail fast rather than hang.
const customLogger = createDaemonAwareLogger(createLogger(), daemon);

export default defineConfig({
  plugins: [react()],
  customLogger,
  server: {
    host,
    port: 5273,
    proxy: {
      "/api": {
        target: daemon,
        changeOrigin: true,
        ws: true,
        ...daemonAuth,
        configure: respondDaemonDown(daemon),
      },
      "/health": {
        target: daemon,
        changeOrigin: true,
        ...daemonAuth,
        configure: respondDaemonDown(daemon),
      },
    },
  },
});
