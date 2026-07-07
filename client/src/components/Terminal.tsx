import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { streamUrl, api } from "../api";
import { Target } from "../connectionStore";
import { useUiStore } from "../store";
import { copyText } from "../clipboard";
import i18n from "../i18n";

/** WS close code the daemon uses when another client takes over the session. */
const CLOSE_SUPERSEDED = 4001;

/**
 * Copy the terminal selection lives on ⌘-C (macOS) and Ctrl-Shift-C
 * (Windows/Linux) so plain Ctrl-C stays SIGINT to the agent. macOS delivers
 * ⌘-C as a native `copy` event; elsewhere the native copy gesture *is* Ctrl-C,
 * so we must not claim it — hence the platform split.
 */
const isMac = /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent);

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
  // The live socket, mirrored out of the effect so the component-scope image
  // upload can inject over it without the effect's listeners depending on it.
  const wsRef = useRef<WebSocket | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const errorTimerRef = useRef<number | undefined>(undefined);
  // Transient status for an in-flight / failed image paste, shown as a small
  // overlay (never written into the terminal, which a TUI would repaint over).
  const [pasteStatus, setPasteStatus] = useState<{ kind: "busy" | "error"; msg: string } | null>(
    null,
  );

  // Upload a pasted/dropped/picked image, then inject its stored path over the
  // live socket as prompt text — the drag-and-drop-equivalent the agent loads
  // on submit. The upload finishes BEFORE the path is injected, so a slow or
  // dropped link never leaves a dangling reference in the prompt. Lifted to
  // component scope so the 📎 button and the terminal's paste/drop listeners
  // share one implementation.
  const uploadAndInject = async (blob: Blob) => {
    if (!live) return;
    setPasteStatus({ kind: "busy", msg: i18n.t("terminal.uploadingImage") });
    try {
      const r = await api.pasteImage(target, sessionId, blob);
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(
          JSON.stringify({ t: "i", d: i18n.t("terminal.pastedImageRef", { path: r.relative_path }) }),
        );
      }
      setPasteStatus(null);
    } catch (e) {
      setPasteStatus({
        kind: "error",
        msg: i18n.t("terminal.pasteFailed", { message: (e as Error).message }),
      });
      if (errorTimerRef.current) window.clearTimeout(errorTimerRef.current);
      errorTimerRef.current = window.setTimeout(() => setPasteStatus(null), 4000);
    }
  };
  // The effect's DOM listeners reach the latest closure through this ref, so
  // they never become an effect dependency (which would rebuild the terminal).
  const uploadRef = useRef(uploadAndInject);
  uploadRef.current = uploadAndInject;

  const onPickFile = (e: ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (f && f.type.startsWith("image/")) void uploadAndInject(f);
    e.target.value = ""; // let the same file be picked again next time
  };

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
        selectionBackground: "#33467c",
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
      wsRef.current = socket;

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
          term.write("\r\n\x1b[33m[" + i18n.t("terminal.takenOver") + "]\x1b[0m\r\n");
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

    const sendInput = (d: string) => {
      if (live && ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: "i", d }));
      }
    };

    const dataSub = term.onData(sendInput);

    // --- Copy selection to the OS clipboard ---
    // Selection requires Shift+drag while a TUI holds the mouse (it captures
    // plain drags for its own mouse reporting); a plain shell selects on drag.
    // On Windows/Linux we claim Ctrl-Shift-C here and leave Ctrl-C to xterm so
    // it still forwards \x03 (SIGINT). macOS ⌘-C arrives as a native `copy`
    // event (handled below), so this handler ignores it.
    term.attachCustomKeyEventHandler((e) => {
      if (
        e.type === "keydown" &&
        !isMac &&
        e.ctrlKey &&
        e.shiftKey &&
        !e.altKey &&
        !e.metaKey &&
        (e.key === "c" || e.key === "C") &&
        term.hasSelection()
      ) {
        void copyText(term.getSelection());
        e.preventDefault();
        return false; // swallow: don't let xterm forward it as input
      }
      return true;
    });

    // macOS ⌘-C (and any browser-native copy gesture) fills the clipboard
    // synchronously from the selection — this path also works in insecure
    // contexts, where navigator.clipboard is unavailable. Gated to macOS so
    // that on Windows/Linux, where the native copy gesture IS Ctrl-C, we don't
    // divert it away from SIGINT.
    const onCopy = (e: ClipboardEvent) => {
      if (!isMac || !term.hasSelection() || !e.clipboardData) return;
      e.clipboardData.setData("text/plain", term.getSelection());
      e.preventDefault();
    };
    // Right-click copies on every platform (the "universal" affordance).
    const onContextMenu = (e: MouseEvent) => {
      if (!term.hasSelection()) return; // nothing selected: leave the default
      void copyText(term.getSelection());
      e.preventDefault();
    };

    // --- Image paste / drop --- (the upload+inject itself lives at component
    // scope in `uploadAndInject`, reached here via `uploadRef` so these
    // listeners don't become effect dependencies; the 📎 button shares it.)
    const firstImage = (files: FileList | null | undefined): File | null =>
      files ? (Array.from(files).find((f) => f.type.startsWith("image/")) ?? null) : null;

    const onPaste = (e: ClipboardEvent) => {
      if (!live || !e.clipboardData) return;
      for (const item of Array.from(e.clipboardData.items)) {
        if (item.kind === "file" && item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) {
            // Swallow it so xterm doesn't also paste garbage; a plain-text
            // paste (no image item) falls through to xterm untouched.
            e.preventDefault();
            e.stopPropagation();
            void uploadRef.current(file);
            return;
          }
        }
      }
    };
    const onDragOver = (e: DragEvent) => {
      if (live && e.dataTransfer && Array.from(e.dataTransfer.items).some((i) => i.kind === "file")) {
        e.preventDefault();
      }
    };
    const onDrop = (e: DragEvent) => {
      if (!live) return;
      const img = firstImage(e.dataTransfer?.files);
      if (img) {
        e.preventDefault();
        void uploadRef.current(img);
      }
    };
    container.addEventListener("copy", onCopy);
    container.addEventListener("contextmenu", onContextMenu);
    container.addEventListener("paste", onPaste, true);
    container.addEventListener("dragover", onDragOver);
    container.addEventListener("drop", onDrop);

    const ro = new ResizeObserver(() => {
      safeFit(fit);
      sendResize();
    });
    ro.observe(container);

    connect();

    return () => {
      mounted = false;
      if (reconnectTimer) window.clearTimeout(reconnectTimer);
      if (errorTimerRef.current) window.clearTimeout(errorTimerRef.current);
      container.removeEventListener("copy", onCopy);
      container.removeEventListener("contextmenu", onContextMenu);
      container.removeEventListener("paste", onPaste, true);
      container.removeEventListener("dragover", onDragOver);
      container.removeEventListener("drop", onDrop);
      ro.disconnect();
      dataSub.dispose();
      wsRef.current = null;
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

  // xterm owns `terminal-mount` imperatively; the overlay is a React-managed
  // sibling so the two never contend over the same DOM children.
  return (
    <div className="terminal-host">
      <div className="terminal-mount" ref={containerRef} />
      {live && (
        <>
          {/* Explicit attach affordance — the primary path on touch devices,
              where clipboard-image paste is unreliable. The 📎 glyph is set in
              CSS so there's no bare string literal in JSX. */}
          <button
            type="button"
            className="term-attach"
            title={i18n.t("terminal.attachImage")}
            aria-label={i18n.t("terminal.attachImage")}
            onClick={() => fileInputRef.current?.click()}
          />
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            hidden
            onChange={onPickFile}
          />
        </>
      )}
      {pasteStatus && (
        <div className={`paste-status paste-status--${pasteStatus.kind}`} role="status">
          {pasteStatus.msg}
        </div>
      )}
    </div>
  );
}

function safeFit(fit: FitAddon) {
  try {
    fit.fit();
  } catch {
    /* container not measured yet */
  }
}
