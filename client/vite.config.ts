import { defineConfig, createLogger } from "vite";
import react from "@vitejs/plugin-react";
import { createDaemonAwareLogger, respondDaemonDown } from "./vite.proxy-log";

// Proxy API + WebSocket traffic to the daemon during development. Point
// ASM_DAEMON at it when it isn't on this machine (or isn't on the default port):
//
//   ASM_DAEMON=https://192.168.0.159:4600 npm run dev
//
// A daemon with ASM_TLS_CERT serves https/wss on its bind — loopback included,
// it is one listener — so `http://127.0.0.1:4600` does not reach it and the
// proxy fails every request with an HTTP/0.9 parse error. Give the scheme the
// daemon actually serves.
const daemon = process.env.ASM_DAEMON ?? "http://127.0.0.1:4600";
const daemonIsTls = daemon.startsWith("https://") || daemon.startsWith("wss://");

// Host the dev server binds to. Default `true` = all interfaces (0.0.0.0 + ::),
// so the React client is reachable from other machines on the LAN — point a
// browser at http://<this-host>:5273. This mirrors binding the daemon to
// 0.0.0.0 (see scripts/wizard.sh). Set ASM_CLIENT_HOST=127.0.0.1 to restrict
// back to loopback only.
// NOTE: when the dev server runs ON the daemon's machine, its proxy dials the
// daemon from 127.0.0.1, so LAN clients reach it over the loopback-trusted path
// (no token) — only bind to the network on a trusted LAN. A dev server on a
// DIFFERENT host (ASM_DAEMON=https://<lan-ip>:4600) dials from its own LAN
// address, which the daemon does not trust: the page is same-origin but the peer
// is not loopback, so the browser enrolls once via Connections → "Enroll this
// device". That is the same path a phone opening the daemon-served UI takes.
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
        // A daemon is normally reached by IP with a self-signed cert, so verify
        // would reject it and there is no interstitial to click on a server-side
        // proxy. This is the dev proxy trusting the daemon it was pointed at —
        // the browser never sees that cert, and it speaks plain http to Vite.
        secure: !daemonIsTls,
        configure: respondDaemonDown(daemon),
      },
      "/health": {
        target: daemon,
        changeOrigin: true,
        secure: !daemonIsTls,
        configure: respondDaemonDown(daemon),
      },
    },
  },
});
