import { useEffect, useRef } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { streamUrl } from "../api";
import { Target } from "../connectionStore";
import { useUiStore } from "../store";

/** WS close code the daemon uses when another client takes over the session. */
const CLOSE_SUPERSEDED = 4001;

interface Props {
  target: Target;
  sessionId: string;
  live: boolean;
}

/**
 * xterm.js terminal bound to one session's WebSocket stream.
 *
 * The server owns terminal history and resume: on attach it sends a snapshot
 * repaint (first binary frame) followed by live output. For live sessions we
 * forward keystrokes and resize; for terminal sessions we render the replayed
 * history read-only. Live sockets auto-reconnect after transient loss.
 */
export function TerminalView({ target, sessionId, live }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const term = new XTerm({
      fontFamily:
        'ui-monospace, SFMono-Regular, Menlo, Monaco, "Cascadia Code", "Roboto Mono", monospace',
      fontSize: 13,
      scrollback: 5000,
      cursorBlink: live,
      theme: {
        background: "#0b0e14",
        foreground: "#c7d0e0",
        cursor: "#7aa2f7",
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    safeFit(fit);

    let mounted = true;
    let ws: WebSocket | null = null;
    let reconnectTimer: number | undefined;

    const sendResize = () => {
      if (live && ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: "r", rows: term.rows, cols: term.cols }));
      }
    };

    const connect = () => {
      if (!mounted) return;
      const socket = new WebSocket(streamUrl(target, sessionId));
      socket.binaryType = "arraybuffer";
      ws = socket;

      socket.onopen = () => {
        // The attach snapshot replays scrollback history. Drop what this
        // terminal already holds so a reconnect doesn't append a second copy.
        // The alternate buffer has no scrollback (and its snapshot carries
        // none), so leave the normal buffer's history alone while a TUI owns
        // the screen.
        if (term.buffer.active.type === "normal") term.clear();
        safeFit(fit);
        sendResize();
      };
      socket.onmessage = (ev) => {
        if (typeof ev.data === "string") term.write(ev.data);
        else term.write(new Uint8Array(ev.data as ArrayBuffer));
      };
      socket.onclose = (ev) => {
        if (ev.code === CLOSE_SUPERSEDED) {
          // Taken over by another client — do NOT reconnect (that would start a
          // takeover ping-pong). Show why, then clear the selection so the
          // session can be reclaimed from the sidebar (which prompts again).
          term.write(
            "\r\n\x1b[33m[This session was taken over by another client.]\x1b[0m\r\n",
          );
          reconnectTimer = window.setTimeout(() => {
            if (mounted) useUiStore.getState().setActive(null);
          }, 1800);
          return;
        }
        if (mounted && live) {
          // Transient loss: reconnect; the snapshot repaints current state.
          reconnectTimer = window.setTimeout(connect, 1000);
        }
      };
      socket.onerror = () => socket.close();
    };

    const dataSub = term.onData((d) => {
      if (live && ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: "i", d }));
      }
    });

    const ro = new ResizeObserver(() => {
      safeFit(fit);
      sendResize();
    });
    ro.observe(container);

    connect();

    return () => {
      mounted = false;
      if (reconnectTimer) window.clearTimeout(reconnectTimer);
      ro.disconnect();
      dataSub.dispose();
      if (ws) {
        ws.onclose = null;
        try {
          ws.close();
        } catch {
          /* ignore */
        }
      }
      term.dispose();
    };
  }, [sessionId, live, target.baseUrl, target.token]);

  return <div className="terminal-host" ref={containerRef} />;
}

function safeFit(fit: FitAddon) {
  try {
    fit.fit();
  } catch {
    /* container not measured yet */
  }
}
