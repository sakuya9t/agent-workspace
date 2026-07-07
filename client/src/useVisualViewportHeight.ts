import { useEffect, useState } from "react";

/**
 * Height (px) of the visual viewport — the area actually visible above the soft
 * keyboard — or `null` when the API is absent (fall back to CSS `100dvh`).
 *
 * The mobile shell drives its height off this so the terminal key bar sits
 * exactly above the keyboard instead of behind it (on iOS the layout viewport
 * doesn't shrink when the keyboard opens). Shrinking the shell shrinks the
 * terminal body, and TerminalView's existing ResizeObserver → fit() → resize
 * chain refits the PTY to the new size.
 */
export function useVisualViewportHeight(): number | null {
  const [height, setHeight] = useState<number | null>(() =>
    typeof window !== "undefined" && window.visualViewport
      ? window.visualViewport.height
      : null,
  );
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return;
    const onResize = () => setHeight(vv.height);
    vv.addEventListener("resize", onResize);
    vv.addEventListener("scroll", onResize);
    onResize();
    return () => {
      vv.removeEventListener("resize", onResize);
      vv.removeEventListener("scroll", onResize);
    };
  }, []);
  return height;
}
