import { useMediaQuery } from "./useMediaQuery";

/**
 * Phone device class. Phones get the stacked mobile shell in **both**
 * orientations: the height clause catches landscape phones (coarse pointer +
 * short viewport) where the 3-pane grid technically fits but leaves an unusable
 * terminal under browser chrome + keyboard. iPad mini portrait (744px) and
 * short desktop windows (never `pointer: coarse`) stay on the desktop shell —
 * matching the "iPad app = desktop web" rule.
 *
 * This is a LAYOUT class, not an input class: an iPad is desktop-shaped but has
 * no keyboard to copy/paste with. For "does this user have a mouse and a
 * keyboard", ask {@link useIsTouch} instead.
 */
export const PHONE_MQ =
  "(max-width: 599px), ((max-height: 599px) and (pointer: coarse))";

/**
 * `true` when the viewport is phone-class. Re-evaluates on the media-query
 * change (rotation / resize); crossing the boundary swaps shells live, and
 * since all state lives in stores/queries nothing is lost.
 */
export function useIsPhone(): boolean {
  return useMediaQuery(PHONE_MQ);
}
