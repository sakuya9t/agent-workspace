import { useCallback } from "react";

interface Props {
  /**
   * Which side panel this handle drags. Dragging right grows a `left` panel and
   * shrinks a `right` one; the center column absorbs the difference.
   */
  side: "left" | "right";
  /** Current width (px) of the adjacent panel. */
  width: number;
  /** Report the panel's new target width (px); the store clamps it. */
  onResize: (px: number) => void;
  /** Accessible label for the separator. */
  label: string;
}

/** Keyboard nudge (px); Shift takes a coarser step. */
const STEP = 16;
const STEP_COARSE = 48;

/**
 * A draggable vertical divider that resizes an adjacent side panel. Sits in the
 * workspace grid as its own thin column so it never overlaps panel content.
 */
export function PanelResizer({ side, width, onResize, label }: Props) {
  const dir = side === "left" ? 1 : -1;

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      const startX = e.clientX;
      const startWidth = width;
      const onMove = (ev: PointerEvent) => {
        onResize(startWidth + dir * (ev.clientX - startX));
      };
      const onUp = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        document.body.classList.remove("col-resizing");
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      document.body.classList.add("col-resizing");
    },
    [dir, width, onResize],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      e.preventDefault();
      const step = e.shiftKey ? STEP_COARSE : STEP;
      const deltaX = e.key === "ArrowRight" ? step : -step;
      onResize(width + dir * deltaX);
    },
    [dir, width, onResize],
  );

  return (
    <div
      className="col-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(width)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    >
      <span className="col-resizer-grip" />
    </div>
  );
}
