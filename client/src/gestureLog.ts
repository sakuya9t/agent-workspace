/**
 * On-device tracer for the terminal's touch gestures. Off unless the URL carries
 * `?gesturelog=1`; when on, it paints the raw event stream and the gesture layer's
 * own decisions (Terminal.tsx) onto an overlay you can read — and copy — on the
 * phone itself.
 *
 * It exists because those gestures can only be exercised on a real touch device,
 * and a real iPhone is the one browser we cannot instrument from here: Safari's
 * Web Inspector needs a Mac, and Chrome's device-emulation mode is not WebKit —
 * it synthesizes pointer events from a mouse and runs none of iOS's own gesture
 * recognizers, so it will happily pass a gesture that iOS kills.
 *
 * What the stream answers:
 *   - do pointer events arrive at all, and with pointerType "touch"?
 *   - does a `pointercancel` land BEFORE the long press (450ms) — i.e. is the
 *     engine claiming the touch for a gesture of its own?
 *   - does a target go `conn=0` mid-gesture — the renderer detaching the row out
 *     from under the finger, so events reach no listener in the tree?
 *   - was a setPointerCapture refused (`▸capture-failed`)?
 */
import { copyText } from "./clipboard";

export const GESTURE_LOG = new URLSearchParams(window.location.search).get("gesturelog") === "1";

const MAX_LINES = 80;
const lines: string[] = [];
let listEl: HTMLElement | null = null;
let t0 = 0;
let lastTag = "";
let lastCount = 0;

/** ms since the gesture began, so a long press reads as "452 ▸select". */
function stamp(): string {
  const now = performance.now();
  if (t0 === 0) t0 = now;
  return `${Math.round(now - t0)}`.padStart(4, " ");
}

function push(tag: string, detail: string): void {
  const time = stamp();
  if (tag === lastTag && lines.length > 0) {
    // A drag is hundreds of moves; collapse a run into one line, so the events
    // bracketing it stay on screen.
    lastCount += 1;
    lines[0] = `${time} ${tag} ×${lastCount} ${detail}`;
  } else {
    lastTag = tag;
    lastCount = 1;
    lines.unshift(`${time} ${tag} ${detail}`); // newest first: nothing to scroll
    if (lines.length > MAX_LINES) lines.pop();
  }
  if (listEl) listEl.textContent = lines.join("\n");
}

/** A note from the gesture layer itself — what it decided, not what it received. */
export function glog(tag: string, detail = ""): void {
  if (!GESTURE_LOG) return;
  push(`▸${tag}`, detail);
}

/** `conn=0` marks a node detached from the document: events keep being dispatched
 *  to it and reach no listener in the tree. */
function describe(target: EventTarget | null): string {
  if (!(target instanceof Element)) return "?";
  const cls = target.classList[0] ? `.${target.classList[0]}` : "";
  return `${target.tagName.toLowerCase()}${cls} conn=${target.isConnected ? 1 : 0}`;
}

/**
 * Start tracing. The listeners are passive and capture-phase, so they observe the
 * gesture without altering it — and, like every other listener in the tree, they
 * fall silent for an event dispatched to a detached target, which is itself one of
 * the things we are here to find out.
 */
export function armGestureLog(): void {
  if (!GESTURE_LOG) return;

  const box = document.createElement("div");
  box.className = "gesture-log";
  const list = document.createElement("pre");
  list.className = "gesture-log-lines";
  const copy = document.createElement("button");
  copy.className = "gesture-log-copy";
  copy.textContent = "copy";
  copy.addEventListener("click", () => {
    // Oldest-first for reading; copyText survives an insecure context (LAN http).
    void copyText(lines.slice().reverse().join("\n")).then((ok) => {
      copy.textContent = ok ? "copied" : "failed";
      window.setTimeout(() => (copy.textContent = "copy"), 1200);
    });
  });
  box.append(copy, list);
  document.body.append(box);
  listEl = list;

  const pointer = (e: PointerEvent) => {
    if (e.type === "pointerdown") t0 = 0; // each gesture's clock starts at its press
    const at = `${Math.round(e.clientX)},${Math.round(e.clientY)}`;
    push(e.type, `${e.pointerType}#${e.pointerId} ${at} ${describe(e.target)}`);
  };
  const touch = (e: TouchEvent) => push(e.type, `n=${e.touches.length} ${describe(e.target)}`);
  const mouse = (e: MouseEvent) => push(e.type, describe(e.target));
  const listen = (types: string[], fn: (e: never) => void) => {
    for (const type of types) {
      window.addEventListener(type, fn as EventListener, { capture: true, passive: true });
    }
  };

  listen(["pointerdown", "pointermove", "pointerup", "pointercancel"], pointer);
  listen(["touchstart", "touchmove", "touchend", "touchcancel"], touch);
  listen(["mousedown", "mouseup", "click", "contextmenu", "selectstart"], mouse);

  push("armed", navigator.userAgent.slice(0, 60));
}
