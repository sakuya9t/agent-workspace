import { useEffect } from "react";

/**
 * Blinks the browser tab title while `count > 0` *and the tab is backgrounded*,
 * so a blocked session that needs the user is noticeable from another tab
 * without nagging while they're actively looking at the app (the in-app badge
 * already covers that case). Alternates between `alertTitle` and `baseTitle` on
 * a ~1s cadence; restores `baseTitle` when the count drops to 0, the tab regains
 * focus, or the component unmounts.
 *
 * `baseTitle` mirrors what i18n's syncDocument sets (`app.title`), so the
 * cleared state matches the normal title exactly — the two never fight.
 */
export function useTabAlert(count: number, alertTitle: string, baseTitle: string) {
  useEffect(() => {
    if (count <= 0) {
      document.title = baseTitle;
      return;
    }

    let timer: ReturnType<typeof setInterval> | undefined;
    let showAlert = false;

    const stop = () => {
      if (timer !== undefined) {
        clearInterval(timer);
        timer = undefined;
      }
      document.title = baseTitle;
    };

    const start = () => {
      if (timer !== undefined) return;
      // Show the alert immediately (don't wait a full tick), then toggle.
      showAlert = true;
      document.title = alertTitle;
      timer = setInterval(() => {
        showAlert = !showAlert;
        document.title = showAlert ? alertTitle : baseTitle;
      }, 1000);
    };

    // Only blink while the tab is hidden; a foregrounded tab shows the badge.
    const sync = () => (document.hidden ? start() : stop());
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => {
      document.removeEventListener("visibilitychange", sync);
      stop();
    };
  }, [count, alertTitle, baseTitle]);
}
