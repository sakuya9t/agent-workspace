import { useMediaQuery } from "./useMediaQuery";

/**
 * Touch-primary device: the user drives this with a finger and has no keyboard
 * to press ⌘-C / Ctrl-V with, so every clipboard path has to be a button.
 *
 * `pointer: coarse` describes the PRIMARY pointer, which is why it's the right
 * test and `any-pointer: coarse` is not: a touchscreen laptop reports a *fine*
 * primary pointer (the trackpad) and keeps the key chords, so it must not get
 * the touch affordances. An iPad reports coarse and gets them — which is the
 * whole point, since it takes the desktop shell (see {@link useIsPhone}) and so
 * never sees the phone key bar's Copy/Paste.
 */
export const TOUCH_MQ = "(pointer: coarse)";

/** `true` when the primary pointer is a finger. Live-updates if that changes. */
export function useIsTouch(): boolean {
  return useMediaQuery(TOUCH_MQ);
}
