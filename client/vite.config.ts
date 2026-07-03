import { defineConfig, createLogger } from "vite";
import react from "@vitejs/plugin-react";
import { createDaemonAwareLogger, respondDaemonDown } from "./vite.proxy-log";

// Proxy API + WebSocket traffic to the local daemon during development.
const daemon = process.env.ASM_DAEMON ?? "http://127.0.0.1:4600";

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
    port: 5273,
    proxy: {
      "/api": {
        target: daemon,
        changeOrigin: true,
        ws: true,
        configure: respondDaemonDown(daemon),
      },
      "/health": {
        target: daemon,
        changeOrigin: true,
        configure: respondDaemonDown(daemon),
      },
    },
  },
});
