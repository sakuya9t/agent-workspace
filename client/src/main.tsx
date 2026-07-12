// i18n must initialize before any module whose code can call t().
import "./i18n";
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { App } from "./App";
import { armGestureLog } from "./gestureLog";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

// No-op unless the URL carries `?gesturelog=1` — the on-device tracer for touch
// gestures, which are the one thing we cannot debug from a desktop.
armGestureLog();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, retry: 1 },
  },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
