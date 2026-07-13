import { useEffect, useRef, useState, type ChangeEvent, type MutableRefObject } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { streamUrl, api, MAX_ATTACHMENT_BYTES } from "../api";
import { Target } from "../connectionStore";
import { useUiStore } from "../store";
import { copyText } from "../clipboard";
import { glog } from "../gestureLog";
import { CtrlLatch, TerminalHandle } from "../terminalTypes";
import i18n from "../i18n";

/** WS close code the daemon uses when another client takes over the session. */
const CLOSE_SUPERSEDED = 4001;

/** Size for a human, only ever used on the attachment limit (always MB-scale). */
const formatBytes = (n: number) => `${(n / (1024 * 1024)).toFixed(1)} MB`;

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
 * hence the platform split. Paste is the mirror image: ⌘-V is already native on
 * macOS, while on Windows/Linux we take Ctrl-V away from xterm (it would send
 * ^V) so the browser's own paste runs — the only paste that carries an image.
 * See the key handler below.
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

  const showError = (msg: string) => {
    setPasteStatus({ kind: "error", msg });
    if (errorTimerRef.current) window.clearTimeout(errorTimerRef.current);
    errorTimerRef.current = window.setTimeout(() => setPasteStatus(null), 4000);
  };

  // Upload a pasted/dropped/picked file of ANY type, then inject its stored path
  // over the live socket as prompt text — the drag-and-drop-equivalent the agent
  // loads on submit. An image is just the common case: a PDF, a zip or a CSV is
  // as useful to the agent, and it reads them from the same path. The upload
  // finishes BEFORE the path is injected, so a slow or dropped link never leaves
  // a dangling reference in the prompt. Lifted to component scope so the 📎
  // button and the terminal's paste/drop listeners share one implementation.
  const uploadAndInject = async (file: Blob, name?: string) => {
    if (!live) return;
    // Pre-check the size the daemon enforces anyway, so a too-big file fails
    // instantly and legibly instead of after a long upload ending in a 413.
    if (file.size > MAX_ATTACHMENT_BYTES) {
      showError(
        i18n.t("terminal.attachTooLarge", {
          size: formatBytes(file.size),
          max: formatBytes(MAX_ATTACHMENT_BYTES),
        }),
      );
      return;
    }
    const isImage = file.type.startsWith("image/");
    setPasteStatus({
      kind: "busy",
      msg: name
        ? i18n.t("terminal.uploadingNamed", { name })
        : i18n.t("terminal.uploadingImage"),
    });
    try {
      const r = await api.uploadAttachment(target, sessionId, file, name);
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        // Keep the verified image wording for images (docs/image-paste.md
        // proved Claude Code loads exactly this form); everything else says
        // "attached file", which is the same bare-path-in-prose shape.
        const key = isImage ? "terminal.pastedImageRef" : "terminal.attachedFileRef";
        ws.send(JSON.stringify({ t: "i", d: i18n.t(key, { path: r.relative_path }) }));
      }
      setPasteStatus(null);
    } catch (e) {
      showError(i18n.t("terminal.pasteFailed", { message: (e as Error).message }));
    }
  };
  // The effect's DOM listeners reach the latest closure through this ref, so
  // they never become an effect dependency (which would rebuild the terminal).
  const uploadRef = useRef(uploadAndInject);
  uploadRef.current = uploadAndInject;

  const onPickFile = (e: ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (f) void uploadAndInject(f, f.name);
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
      // Defaults ON for macOS, where it quietly REPLACES the user's selection
      // with the word under the pointer on right-click — and the context-menu
      // copy below then faithfully "succeeds" on that single word.
      rightClickSelectsWord: false,
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
    const core = (
      term as unknown as {
        _core?: {
          _selectionService?: {
            shouldForceSelection?: (e: MouseEvent) => boolean;
            clearSelection?: () => void;
            disable?: () => void;
          };
          coreMouseService?: {
            triggerMouseEvent?: (ev: { button: number; action: number }) => boolean;
          };
          coreService?: {
            triggerDataEvent?: (data: string, wasUserInput?: boolean) => void;
          };
        };
      }
    )._core;
    const selection = core?._selectionService;
    if (selection?.shouldForceSelection) {
      const shouldForceSelection = selection.shouldForceSelection.bind(selection);
      selection.shouldForceSelection = (e: MouseEvent) => e.shiftKey || shouldForceSelection(e);
    }

    // WORKAROUND (@xterm/xterm 5.5.0): a Shift+drag selection under mouse
    // reporting died before it could be copied, three ways — all downstream of
    // "any user input clears the selection", where a report TO the app counts
    // as user input:
    //  1. Under ?1003h (any-motion tracking — Claude Code runs this) merely
    //     MOVING the mouse after releasing the drag sends a motion report.
    //  2. A wheel scroll or a focus in/out report (?1004h) does the same.
    //  3. Re-asserting mouse modes — which Claude Code does on every redraw,
    //     spinner ticks included — fires onProtocolChange (the setter doesn't
    //     dedupe same-value writes), whose handler disable()s the selection
    //     service, and disable() clears too.
    // Suppress the clear for exactly those synchronous paths. Real button
    // presses still dismiss the highlight, and real keystrokes still clear via
    // the keyboard path, so click-to-deselect UX is unchanged.
    const coreMouse = core?.coreMouseService;
    const coreSvc = core?.coreService;
    if (
      selection?.clearSelection &&
      selection.disable &&
      coreMouse?.triggerMouseEvent &&
      coreSvc?.triggerDataEvent
    ) {
      let passiveInput = false;
      const guard = <A extends unknown[], R>(fn: (...args: A) => R, isPassive: (...args: A) => boolean) => {
        return (...args: A): R => {
          const prev = passiveInput;
          passiveInput = prev || isPassive(...args);
          try {
            return fn(...args);
          } finally {
            passiveInput = prev;
          }
        };
      };
      coreMouse.triggerMouseEvent = guard(
        coreMouse.triggerMouseEvent.bind(coreMouse),
        (ev) => ev.action === 32 /* move */ || ev.button === 4 /* wheel */,
      );
      coreSvc.triggerDataEvent = guard(
        coreSvc.triggerDataEvent.bind(coreSvc),
        (data) => data === "\x1b[I" || data === "\x1b[O", // focus reports
      );
      selection.disable = guard(selection.disable.bind(selection), () => true);
      const clearSelection = selection.clearSelection.bind(selection);
      selection.clearSelection = () => {
        if (!passiveInput) clearSelection();
      };
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

    // Copy: on Windows/Linux we claim Ctrl-Shift-C here and leave Ctrl-C to xterm
    // so it still forwards \x03 (SIGINT). macOS ⌘-C needs nothing from us: xterm
    // doesn't cancel the ⌘-C keydown, so the browser fires a native `copy` event
    // that xterm's own listener serves from the selection — synchronously, which
    // also covers insecure contexts.
    //
    // Paste: only the browser's *native* paste carries an image — the image lives
    // in the `paste` event's clipboardData, which `onPaste` below uploads. So a
    // paste gesture is useful to us only if it reaches the browser uncancelled.
    // macOS ⌘-V does, which is why image paste worked there and nowhere else:
    //   - plain Ctrl-V: xterm maps it to ^V and preventDefault()s, so Chrome
    //     skips its paste command and NO paste event ever fires;
    //   - Ctrl-Shift-V: Chrome's *paste-as-plain-text*, whose clipboardData is
    //     stripped to text — an image-only clipboard arrives empty.
    // Windows was therefore left with no way to paste an image at all. So claim
    // Ctrl-V and hand it straight back to the browser. This spends ^V (readline's
    // quoted-insert, vim's visual-block) on Windows/Linux, the same trade Windows
    // Terminal and VS Code's terminal make — vim's own Ctrl-Q is the way back.
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || isMac || !e.ctrlKey || e.altKey || e.metaKey) return true;
      if (e.shiftKey && (e.key === "c" || e.key === "C") && term.hasSelection()) {
        void copySelection();
        e.preventDefault();
        return false; // swallow: don't let xterm forward it as input
      }
      if (!e.shiftKey && (e.key === "v" || e.key === "V")) {
        // Deliberately no preventDefault: xterm must not send ^V, but the
        // browser MUST still run its paste command — that is what fires the
        // `paste` event (image → onPaste; text → xterm's own paste listener).
        return false;
      }
      return true;
    });

    // A right-click on a selection must never be REPORTED to a mouse-tracking
    // TUI: xterm clears the selection on any user input — and the report IS
    // user input — wiping it before contextmenu fires, so the copy below would
    // read "". Swallow the press in capture phase (xterm's bubble listeners on
    // a descendant never see it); the browser still fires contextmenu.
    const onMouseDownCapture = (e: MouseEvent) => {
      // A touch selection has just been made: this is the compatibility mousedown
      // the browser synthesizes from the lift, and xterm would clear on it.
      if (holdSelection) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (e.button === 2 && term.hasSelection()) e.stopPropagation();
    };
    // Right-click copies on every platform (the "universal" affordance), then
    // clears the selection so the NEXT right-click falls through to the
    // browser menu — whose Paste works even in insecure contexts (xterm parks
    // its hidden textarea under the cursor for exactly that).
    const onContextMenu = (e: MouseEvent) => {
      // A long-press can raise contextmenu too; that's our selection gesture, not
      // a right-click, so swallow it rather than copy-and-clear what it just made.
      if (gesture === "select" || holdSelection) {
        e.preventDefault();
        return;
      }
      if (!term.hasSelection()) return; // nothing selected: leave the default
      void copySelection();
      term.clearSelection();
      e.preventDefault();
    };

    // --- File paste / drop --- (the upload+inject itself lives at component
    // scope in `uploadAndInject`, reached here via `uploadRef` so these
    // listeners don't become effect dependencies; the 📎 button shares it.)
    // Any file type is taken, not just images — `dragover` already advertised
    // that by accepting any file kind, and now `drop` actually honours it.
    const onPaste = (e: ClipboardEvent) => {
      if (!live || !e.clipboardData) return;
      for (const item of Array.from(e.clipboardData.items)) {
        if (item.kind === "file") {
          const file = item.getAsFile();
          if (file) {
            // Swallow it so xterm doesn't also paste garbage; a plain-text
            // paste (whose items are all `kind: "string"`) falls through to
            // xterm untouched.
            e.preventDefault();
            e.stopPropagation();
            // A clipboard image has no meaningful name (Chrome calls it
            // "image.png"); a file copied from a file manager does.
            void uploadRef.current(file, file.name || undefined);
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
      const file = e.dataTransfer?.files?.[0];
      if (file) {
        e.preventDefault();
        void uploadRef.current(file, file.name || undefined);
      }
    };
    // --- Touch: drag scrolls, long-press selects, drag-after-press extends ---
    //
    // Scroll: xterm's built-in touch handler only nudges its own scrollback
    // viewport, so on a TUI (alternate screen, or mouse reporting on) a swipe did
    // nothing — no scrollback to move, and the gesture never reached the app.
    // Desktop escapes this because the *wheel* path forwards to the app (wheel
    // reports, or ↑/↓ for a no-scrollback screen), so translate a vertical drag
    // into wheel events aimed at xterm's element and the identical wheel logic
    // runs. xterm's own touch scroll is suppressed (capture-phase
    // stopPropagation) so it can't double-scroll.
    //
    // Selection: xterm has NO touch selection — its selection service is
    // mouse-only and `.xterm` carries `user-select: none`, so neither xterm nor
    // the browser would ever select a cell from a fingertip. Synthesize the
    // gesture and push it through xterm's own selection model via term.select(),
    // which is what makes the existing copy paths (the key bar's Copy,
    // getSelection(), the Ctrl-Shift-C/right-click handlers) work unchanged.
    //
    // Both ride on POINTER events, not touch events, because the DOM renderer
    // REPLACES a row's <span>s whenever that row repaints. A touch that started
    // on one of those spans is retargeted to a node that is no longer in the
    // tree, so its touchmoves silently stop propagating to any listener — no
    // touchcancel, just nothing. Scrolling repaints rows, so a drag beginning on
    // TEXT scrolled exactly one row and then died, while a drag beginning on
    // blank space (whose target is the row <div> — recycled, not replaced)
    // scrolled normally. setPointerCapture pins the gesture to the container, so
    // what happens to the element under the finger stops mattering.
    const LONG_PRESS_MS = 450;
    const TOUCH_SLOP = 8; // px of travel before a drag is a scroll, not a press
    const EDGE = 24; // px from an edge where an extend-drag auto-scrolls
    /** Word chars stop at these — xterm's own `wordSeparator` default, so a
     *  long-press grabs the same word a desktop double-click would. */
    const WORD_SEPARATORS = " ()[]{}'\"`";

    type Gesture = "idle" | "scroll" | "select";
    let gesture: Gesture = "idle";
    let pointerId: number | null = null;
    let startX = 0;
    let startY = 0;
    let lastX = 0;
    let lastY = 0;
    let pressTimer: number | undefined;
    let autoScrollTimer: number | undefined;
    /** The long-pressed word, in absolute buffer cells; a drag extends off it in
     *  either direction. Half-open: [col,row) → [endCol,endRow). */
    let anchor: { col: number; row: number; endCol: number; endRow: number } | null = null;
    /** True while a just-made touch selection is settling, so the compatibility
     *  mouse events the browser fires after the finger lifts don't reach xterm's
     *  selection service — which clears the selection on any mousedown. */
    let holdSelection = false;
    let holdTimer: number | undefined;

    const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));
    const screenEl = () => term.element?.querySelector(".xterm-screen") as HTMLElement | null;

    /** Client px → absolute buffer cell (scrollback included), clamped to the screen. */
    const cellAt = (x: number, y: number) => {
      const el = screenEl();
      if (!el) return null;
      const r = el.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) return null;
      const col = clamp(Math.floor(((x - r.left) / r.width) * term.cols), 0, term.cols - 1);
      const viewRow = clamp(Math.floor(((y - r.top) / r.height) * term.rows), 0, term.rows - 1);
      return { col, row: term.buffer.active.viewportY + viewRow };
    };

    /** The word spanning `col` on buffer line `row`, or that single cell if the
     *  press landed on whitespace//a separator. */
    const wordAt = (col: number, row: number) => {
      const line = term.buffer.active.getLine(row);
      if (!line) return null;
      const isWord = (i: number) => {
        const cell = line.getCell(i);
        if (!cell) return false;
        if (cell.getWidth() === 0) return true; // trailing half of a wide (CJK) char
        const ch = cell.getChars();
        return ch !== "" && !WORD_SEPARATORS.includes(ch);
      };
      if (!isWord(col)) return { col, row, endCol: col + 1, endRow: row };
      let s = col;
      let e = col;
      while (s > 0 && isWord(s - 1)) s--;
      while (e < term.cols - 1 && isWord(e + 1)) e++;
      return { col: s, row, endCol: e + 1, endRow: row };
    };

    // term.select(col, row, length) walks `length` cells rightward from the start,
    // wrapping by `cols` — so a selection is just a half-open run of absolute cell
    // offsets, and dragging backward past the anchor simply swaps the two ends.
    const offOf = (col: number, row: number) => row * term.cols + col;
    const selectTo = (cur: { col: number; row: number }) => {
      if (!anchor) return;
      const aStart = offOf(anchor.col, anchor.row);
      const aEnd = offOf(anchor.endCol, anchor.endRow); // exclusive
      const c = offOf(cur.col, cur.row);
      const [start, end] =
        c + 1 > aEnd ? [aStart, c + 1] // dragged forward, past the word
        : c < aStart ? [c, aEnd] // dragged backward, before the word
        : [aStart, aEnd]; // still inside the word
      term.select(start % term.cols, Math.floor(start / term.cols), end - start);
    };

    /** Pin the gesture to the container: from here the finger can wander over rows
     *  the renderer is busy replacing, and what becomes of the element under it
     *  stops mattering.
     *
     *  Best-effort, and deliberately NOT on the critical path. Capture is the part
     *  of this gesture an engine is most likely to refuse — it is only defined for
     *  an *active* pointer, and the long-press timer fires outside any pointer
     *  event handler. Blink allows a capture from there (which is all Chrome's
     *  device emulation can ever prove), other engines need not, and the throw used
     *  to escape beginSelect() BEFORE it selected anything: a "long press does
     *  nothing" of exactly the kind iOS reports. So select the word first, attempt
     *  the capture in a try/catch, and retry it from the next pointermove — a
     *  handler no engine objects to. */
    const grabPointer = () => {
      if (pointerId === null || container.hasPointerCapture(pointerId)) return;
      try {
        container.setPointerCapture(pointerId);
      } catch (e) {
        glog("capture-failed", (e as Error).name);
      }
    };

    const beginSelect = () => {
      const cell = cellAt(startX, startY);
      const word = cell && wordAt(cell.col, cell.row);
      if (!cell || !word) return;
      gesture = "select";
      anchor = word;
      selectTo(cell); // first: a refused capture must not cost us the word
      grabPointer();
      navigator.vibrate?.(8); // the only feedback that the press "took" (iOS has none)
      glog("select", `${word.col},${word.row} "${term.getSelection().slice(0, 24)}"`);
    };

    // Extending past the top/bottom edge scrolls, so a selection can run beyond
    // one screenful. Only armed while a selection drag is live.
    const autoScrollTick = () => {
      if (gesture !== "select") return;
      const el = screenEl();
      if (!el) return;
      const r = el.getBoundingClientRect();
      const dir = lastY < r.top + EDGE ? -1 : lastY > r.bottom - EDGE ? 1 : 0;
      if (dir === 0) return;
      term.scrollLines(dir);
      const cell = cellAt(lastX, lastY);
      if (cell) selectTo(cell);
    };

    const endGesture = () => {
      if (pointerId !== null && container.hasPointerCapture(pointerId)) {
        container.releasePointerCapture(pointerId);
      }
      window.clearTimeout(pressTimer);
      window.clearInterval(autoScrollTimer);
      autoScrollTimer = undefined;
      gesture = "idle";
      pointerId = null;
      anchor = null;
    };

    const onPointerDown = (e: PointerEvent) => {
      if (e.pointerType !== "touch") return; // mouse/pen keep xterm's own path
      if (pointerId !== null) {
        endGesture(); // a second finger (pinch/zoom): abandon, own nothing
        return;
      }
      pointerId = e.pointerId;
      gesture = "idle";
      startX = lastX = e.clientX;
      startY = lastY = e.clientY;
      pressTimer = window.setTimeout(beginSelect, LONG_PRESS_MS);
    };

    const onPointerMove = (e: PointerEvent) => {
      if (e.pointerId !== pointerId) return;
      const x = e.clientX;
      const y = e.clientY;

      if (gesture === "select") {
        grabPointer(); // if the timer's capture was refused, this handler's won't be
        lastX = x;
        lastY = y;
        const cell = cellAt(x, y);
        if (cell) selectTo(cell);
        autoScrollTimer ??= window.setInterval(autoScrollTick, 90);
        e.preventDefault();
        return;
      }

      if (gesture === "idle") {
        if (Math.abs(y - startY) < TOUCH_SLOP && Math.abs(x - startX) < TOUCH_SLOP) {
          return; // still inside the slop — the long press is still on the table
        }
        // It moved before the press landed, so this is a scroll, not a selection.
        window.clearTimeout(pressTimer);
        gesture = "scroll";
        grabPointer();
        lastX = x;
        lastY = y; // rebase, so the first step isn't a jump of the whole slop
        glog("scroll");
        e.preventDefault();
        return;
      }

      const deltaY = lastY - y; // finger down → deltaY<0 → toward older, like a wheel
      lastX = x;
      lastY = y;
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
      e.preventDefault();
    };

    const onPointerUp = (e: PointerEvent) => {
      if (e.pointerId !== pointerId) return;
      // A `pointercancel` landing here with gesture=idle is the engine claiming the
      // touch for a gesture of its own, before our long press could land.
      glog(e.type === "pointercancel" ? "cancel" : "up", `gesture=${gesture}`);
      if (gesture === "select") {
        // Keep the highlight — the key bar's Copy reads it. Swallow the mouse
        // events the browser synthesizes from the lift for ~a beat, or xterm's
        // selection service would clear what we just selected.
        holdSelection = true;
        window.clearTimeout(holdTimer);
        holdTimer = window.setTimeout(() => (holdSelection = false), 700);
        e.preventDefault();
      } else if (gesture === "idle" && term.hasSelection()) {
        term.clearSelection(); // a plain tap dismisses the selection
      }
      endGesture();
    };

    // Only the suppression lives on touch events; the gesture itself is on
    // pointer events (see above). Once the renderer detaches the touch's target
    // these stop firing — but xterm's own touch listener is starved by exactly
    // the same detachment, so nothing is left to double-scroll.
    const onTouchMove = (e: TouchEvent) => {
      e.stopPropagation(); // xterm's built-in touch scroll must never run
      if (gesture !== "idle") e.preventDefault(); // ...nor a page bounce/overscroll
    };
    const onTouchEnd = (e: TouchEvent) => {
      // Suppress the synthetic mousedown/click that would clear a fresh selection.
      // (Checked both ways because pointerup/touchend ordering isn't worth betting on.)
      if (gesture === "select" || holdSelection) e.preventDefault();
    };

    container.addEventListener("mousedown", onMouseDownCapture, true);
    container.addEventListener("contextmenu", onContextMenu);
    container.addEventListener("paste", onPaste, true);
    container.addEventListener("dragover", onDragOver);
    container.addEventListener("drop", onDrop);
    container.addEventListener("pointerdown", onPointerDown, true);
    container.addEventListener("pointermove", onPointerMove, { capture: true, passive: false });
    container.addEventListener("pointerup", onPointerUp, true);
    container.addEventListener("pointercancel", onPointerUp, true);
    container.addEventListener("touchmove", onTouchMove, { capture: true, passive: false });
    container.addEventListener("touchend", onTouchEnd, { capture: true, passive: false });

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
      container.removeEventListener("pointerdown", onPointerDown, true);
      container.removeEventListener("pointermove", onPointerMove, { capture: true });
      container.removeEventListener("pointerup", onPointerUp, true);
      container.removeEventListener("pointercancel", onPointerUp, true);
      container.removeEventListener("touchmove", onTouchMove, { capture: true });
      container.removeEventListener("touchend", onTouchEnd, { capture: true });
      window.clearTimeout(pressTimer);
      window.clearTimeout(holdTimer);
      window.clearInterval(autoScrollTimer);
      ro.disconnect();
      dataSub.dispose();
      wsRef.current = null;
      if (ws) {
        const socket = ws;
        socket.onclose = null;
        socket.onmessage = null;
        socket.onerror = null;
        if (socket.readyState === WebSocket.CONNECTING) {
          // close() mid-handshake makes the browser log "WebSocket is closed
          // before the connection is established" (StrictMode's dev double-
          // mount and fast session switches both land here) — let the
          // handshake finish, then close.
          socket.onopen = () => socket.close();
        } else {
          try {
            socket.close();
          } catch {
            /* ignore */
          }
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
            title={i18n.t("terminal.attachFile")}
            aria-label={i18n.t("terminal.attachFile")}
            onClick={() => fileInputRef.current?.click()}
          />
          {/* No `accept` — any file type is a valid attachment (PDF, zip, CSV,
              …), bounded by size alone. */}
          <input ref={fileInputRef} type="file" hidden onChange={onPickFile} />
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
