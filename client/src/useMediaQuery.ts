import { useEffect, useState } from "react";

/** `true` while `query` matches, re-evaluated on every media-query change
 *  (rotation, resize, a pointer being attached). Shared by the device-class
 *  hooks so they can't drift apart in how they subscribe. */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(
    () => typeof window !== "undefined" && window.matchMedia(query).matches,
  );
  useEffect(() => {
    const mq = window.matchMedia(query);
    const onChange = () => setMatches(mq.matches);
    setMatches(mq.matches); // `query` may have changed since the last render
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [query]);
  return matches;
}
