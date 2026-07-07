import { useEffect, useState } from "react";

/**
 * Phone device class. Phones get the stacked mobile shell in **both**
 * orientations: the height clause catches landscape phones (coarse pointer +
 * short viewport) where the 3-pane grid technically fits but leaves an unusable
 * terminal under browser chrome + keyboard. iPad mini portrait (744px) and
 * short desktop windows (never `pointer: coarse`) stay on the desktop shell —
 * matching the "iPad app = desktop web" rule.
 */
export const PHONE_MQ =
  "(max-width: 599px), ((max-height: 599px) and (pointer: coarse))";

/**
 * `true` when the viewport is phone-class. Re-evaluates on the media-query
 * change (rotation / resize); crossing the boundary swaps shells live, and
 * since all state lives in stores/queries nothing is lost.
 */
export function useIsPhone(): boolean {
  const [isPhone, setIsPhone] = useState(
    () => typeof window !== "undefined" && window.matchMedia(PHONE_MQ).matches,
  );
  useEffect(() => {
    const mq = window.matchMedia(PHONE_MQ);
    const onChange = () => setIsPhone(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return isPhone;
}
