import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Proxy API + WebSocket traffic to the local daemon during development.
const daemon = process.env.ASM_DAEMON ?? "http://127.0.0.1:4600";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5273,
    proxy: {
      "/api": { target: daemon, changeOrigin: true, ws: true },
      "/health": { target: daemon, changeOrigin: true },
    },
  },
});
