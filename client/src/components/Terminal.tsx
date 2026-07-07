import { useEffect, useRef, useState, type ChangeEvent, type MutableRefObject } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { streamUrl, api } from "../api";
import { Target } from "../connectionStore";
import { useUiStore } from "../store";
import { copyText } from "../clipboard";
import { CtrlLatch, TerminalHandle } from "../terminalTypes";
import i18n from "../i18n";

/** WS close code the daemon uses when another client takes over the session. */
const CLOSE_SUPERSEDED = 4001;

/** Map a single typed character to its control byte (Ctrl-A → \x01, etc.); pass
 *  anything else (multi-char sequences, non-mappable keys) through untouched. */
function toCtrl(s: string): string {
  if (s.length !== 1) return s;
  const c = s.charCodeAt(0);
  if (c >= 0x61 && c <= 0x7a) return String.fromCharCode(c - 0x60); // a-z → ^A-^Z
  if (c >= 0x40 && c <= 0x5f) return String.fromCharCode(c - 0x40); // @A-Z[\]^_ → ^@-^_
  if (c === 0x20) return "\x00"; // space → ^@ (NUL)
  return s;
}

/**
 * Copying the terminal selection lives on ⌘-C (macOS) and Ctrl-Shift-C
 * (Windows/Linux) so plain Ctrl-C stays SIGINT to the agent. macOS ⌘-C is
 * served natively by xterm's own `copy`-event listener; elsewhere the native
 * copy gesture *is* Ctrl-C, so we claim the Ctrl-Shift-C chord instead —
 * hence the platform split.
 */
const isMac = /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent);

interface Props {
  target: Target;
  sessionId: string;
  live: boolean;
  /** Mobile: receive an imperative handle when the terminal mounts, null on
   *  unmount, so a key bar can inject input over the same WS path. */
  onReady?: (handle: TerminalHandle | null) => void;
  /** Mobile Ctrl latch, read on each typed key; when armed/locked the next
   *  soft-keyboard key is transformed to its control code. */
  ctrlRef?: MutableRefObject<CtrlLatch>;
  /** Called after an "armed" one-shot latch is consumed, so the bar resets. */
  onCtrlConsumed?: () => void;
}

/**
 * xterm.js terminal bound to one session's WebSocket stream.
 *
 * The server owns terminal history and resume: on attach it sends a snapshot
 * repaint (first binary frame) followed by live output. For live sessions we
 * forward keystrokes and resize; for terminal sessions we render the replayed
 * history read-only. Live sockets auto-reconnect after transient loss.
 */
export function TerminalView({
  target,
  sessionId,
  live,
  onReady,
  ctrlRef,
  onCtrlConsumed,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  // Reached from inside the WS effect without becoming effect dependencies
  // (which would tear down and rebuild the terminal on every render).
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  const onCtrlConsumedRef = useRef(onCtrlConsumed);
  onCtrlConsumedRef.current = onCtrlConsumed;
  // The live socket, mirrored out of the effect so the component-scope image
  // upload can inject over it without the effect's listeners depending on it.
  const wsRef = useRef<WebSocket | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const errorTimerRef = useRef<number | undefined>(undefined);
  // Transient status for an in-flight / failed image paste or a copy receipt,
  // shown as a small overlay (never written into the terminal, which a TUI
  // would repaint over).
  const [pasteStatus, setPasteStatus] = useState<{
    kind: "busy" | "ok" | "error";
    msg: string;
  } | null>(null);

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
      // macOS-native gesture for selecting while a TUI holds mouse reporting
      // (Shift+drag is also honored — see the shouldForceSelection patch below).
      macOptionClickForcesSelection: true,
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

    // WORKAROUND (@xterm/xterm 5.5.0): the Viewport schedules deferred
    // syncScrollArea() calls (a post-open setTimeout, plus dimensions-change and
    // sync-scrollbar events). If the terminal is torn down while one is still
    // queued — StrictMode's dev double-mount, or a live/session switch or a
    // reconnect in prod — it runs after dispose and reads renderService
    // .dimensions, whose renderer is already gone, throwing "Cannot read
    // properties of undefined (reading 'dimensions')". Swallow that post-dispose
    // throw; the terminal is gone, so there is nothing left to sync.
    const viewport = (
      term as unknown as { _core?: { viewport?: { syncScrollArea?: (bufferChanged?: boolean) => void } } }
    )._core?.viewport;
    if (viewport?.syncScrollArea) {
      const syncScrollArea = viewport.syncScrollArea.bind(viewport);
      viewport.syncScrollArea = (bufferChanged?: boolean) => {
        try {
          syncScrollArea(bufferChanged);
        } catch {
          /* fired after dispose — safe to ignore */
        }
      };
    }

    // WORKAROUND (@xterm/xterm 5.5.0): while a TUI holds mouse reporting the
    // selection service is disabled, and only a "forced" selection can happen.
    // Upstream forces on Shift+drag everywhere EXCEPT macOS, where it only
    // offers Option+drag (behind macOptionClickForcesSelection) — so on a Mac,
    // Shift+drag over an agent's TUI selected nothing at all. We document
    // Shift+drag as THE selection gesture, so widen the predicate to accept
    // Shift on every platform. The mouse-reporting mousedown path consults the
    // same predicate, so a forced drag is also kept away from the app.
    const selection = (
      term as unknown as {
        _core?: { _selectionService?: { shouldForceSelection?: (e: MouseEvent) => boolean } };
      }
    )._core?._selectionService;
    if (selection?.shouldForceSelection) {
      const shouldForceSelection = selection.shouldForceSelection.bind(selection);
      selection.shouldForceSelection = (e: MouseEvent) => e.shiftKey || shouldForceSelection(e);
    }

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

    // Raw send — used by the key bar handle (explicit control codes) and as the
    // base for typed input.
    const sendRaw = (d: string) => {
      if (live && ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: "i", d }));
      }
    };
    // Soft-keyboard input honors the mobile Ctrl latch; an "armed" one-shot is
    // consumed after a single key (key-bar buttons send raw and bypass this).
    const sendTyped = (d: string) => {
      const latch = ctrlRef?.current;
      if (latch && latch !== "off") {
        if (latch === "armed") onCtrlConsumedRef.current?.();
        sendRaw(toCtrl(d));
      } else {
        sendRaw(d);
      }
    };

    const dataSub = term.onData(sendTyped);

    // --- Copy selection to the OS clipboard ---
    // Selection requires Shift+drag while a TUI holds the mouse (it captures
    // plain drags for its own mouse reporting); a plain shell selects on drag.
    // The clipboard has no observable state, so flash the outcome over the
    // terminal — a silent success is indistinguishable from "copy is broken".
    const copySelection = async () => {
      const ok = await copyText(term.getSelection());
      setPasteStatus({
        kind: ok ? "ok" : "error",
        msg: i18n.t(ok ? "terminal.copied" : "terminal.copyFailed"),
      });
      if (errorTimerRef.current) window.clearTimeout(errorTimerRef.current);
      errorTimerRef.current = window.setTimeout(() => setPasteStatus(null), ok ? 1500 : 4000);
    };

    // On Windows/Linux we claim Ctrl-Shift-C here and leave Ctrl-C to xterm so
    // it still forwards \x03 (SIGINT). macOS ⌘-C needs nothing from us: xterm
    // doesn't cancel the ⌘-C keydown, so the browser fires a native `copy`
    // event that xterm's own listener serves from the selection — synchronously,
    // which also covers insecure contexts. (Paste is native too: ⌘-V and
    // Ctrl-Shift-V keydowns pass through xterm uncancelled, and the browser's
    // paste event lands on xterm's textarea. Plain Ctrl-V stays ^V to the app.)
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
        void copySelection();
        e.preventDefault();
        return false; // swallow: don't let xterm forward it as input
      }
      return true;
    });

    // A right-click on a selection must never be REPORTED to a mouse-tracking
    // TUI: xterm clears the selection on any user input — and the report IS
    // user input — wiping it before contextmenu fires, so the copy below would
    // read "". Swallow the press in capture phase (xterm's bubble listeners on
    // a descendant never see it); the browser still fires contextmenu.
    const onMouseDownCapture = (e: MouseEvent) => {
      if (e.button === 2 && term.hasSelection()) e.stopPropagation();
    };
    // Right-click copies on every platform (the "universal" affordance), then
    // clears the selection so the NEXT right-click falls through to the
    // browser menu — whose Paste works even in insecure contexts (xterm parks
    // its hidden textarea under the cursor for exactly that).
    const onContextMenu = (e: MouseEvent) => {
      if (!term.hasSelection()) return; // nothing selected: leave the default
      void copySelection();
      term.clearSelection();
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
    // --- Touch scroll ---
    // xterm's built-in touch handler only nudges its own scrollback viewport, so
    // on a TUI (an alternate-screen app, or anything with mouse reporting on) a
    // swipe does nothing: there is no scrollback to move and the gesture is never
    // forwarded to the app. Desktop avoids this because the *wheel* path forwards
    // to the app — mouse-wheel reports, or ↑/↓ for a no-scrollback screen. Mirror
    // that on touch: translate a vertical drag into wheel events aimed at xterm's
    // element so the identical wheel logic runs (smooth scrollback for a shell,
    // app-forwarded scroll for a TUI). Registered in the capture phase with
    // stopPropagation so xterm's own touch handler on `.xterm` never also fires
    // and double-scrolls.
    let touchY: number | null = null;
    let touchScrolling = false;
    const TOUCH_SLOP = 6; // px of travel before a drag counts as a scroll (not a tap)
    const onTouchStart = (e: TouchEvent) => {
      touchScrolling = false;
      touchY = e.touches.length === 1 ? e.touches[0].clientY : null;
    };
    const onTouchMove = (e: TouchEvent) => {
      if (touchY === null || e.touches.length !== 1) return; // ignore taps / pinch
      const y = e.touches[0].clientY;
      if (!touchScrolling) {
        if (Math.abs(y - touchY) < TOUCH_SLOP) return; // still might be a tap
        touchScrolling = true;
        touchY = y; // rebase so the first step isn't a jump of the whole slop
      }
      const deltaY = touchY - y; // finger down → deltaY<0 → scroll toward older, like a wheel
      touchY = y;
      if (deltaY !== 0) {
        term.element?.dispatchEvent(
          new WheelEvent("wheel", {
            deltaY,
            deltaMode: WheelEvent.DOM_DELTA_PIXEL,
            bubbles: true,
            cancelable: true,
          }),
        );
      }
      e.preventDefault(); // we own this gesture — suppress page bounce/overscroll
      e.stopPropagation(); // and xterm's built-in touch scroll must not also run
    };
    const onTouchEnd = () => {
      touchY = null;
      touchScrolling = false;
    };

    container.addEventListener("mousedown", onMouseDownCapture, true);
    container.addEventListener("contextmenu", onContextMenu);
    container.addEventListener("paste", onPaste, true);
    container.addEventListener("dragover", onDragOver);
    container.addEventListener("drop", onDrop);
    container.addEventListener("touchstart", onTouchStart, { capture: true, passive: true });
    container.addEventListener("touchmove", onTouchMove, { capture: true, passive: false });
    container.addEventListener("touchend", onTouchEnd, { capture: true });
    container.addEventListener("touchcancel", onTouchEnd, { capture: true });

    const ro = new ResizeObserver(() => {
      safeFit(fit);
      sendResize();
    });
    ro.observe(container);

    connect();

    // Hand a fresh input handle to the shell (the mobile key bar reads it).
    onReadyRef.current?.({
      write: sendRaw,
      focus: () => term.focus(),
      getSelection: () => term.getSelection(),
    });

    return () => {
      mounted = false;
      onReadyRef.current?.(null);
      if (reconnectTimer) window.clearTimeout(reconnectTimer);
      if (errorTimerRef.current) window.clearTimeout(errorTimerRef.current);
      container.removeEventListener("mousedown", onMouseDownCapture, true);
      container.removeEventListener("contextmenu", onContextMenu);
      container.removeEventListener("paste", onPaste, true);
      container.removeEventListener("dragover", onDragOver);
      container.removeEventListener("drop", onDrop);
      container.removeEventListener("touchstart", onTouchStart, { capture: true });
      container.removeEventListener("touchmove", onTouchMove, { capture: true });
      container.removeEventListener("touchend", onTouchEnd, { capture: true });
      container.removeEventListener("touchcancel", onTouchEnd, { capture: true });
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
