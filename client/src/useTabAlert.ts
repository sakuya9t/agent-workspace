import { useEffect } from "react";

/**
 * Swaps every `<link rel="icon">` that declares a `data-alert-href` warning
 * variant (see index.html) between that variant and its original href. The
 * original is stashed on the element the first time it's swapped, so the
 * mapping lives entirely in the markup.
 */
function setAlertIcons(alert: boolean) {
  const links = document.querySelectorAll<HTMLLinkElement>(
    'link[rel="icon"][data-alert-href]',
  );
  links.forEach((link) => {
    const base = (link.dataset.baseHref ??= link.getAttribute("href") ?? "");
    const next = alert ? link.dataset.alertHref ?? base : base;
    // Only touch href on a real change: rewriting it makes browsers refetch
    // and re-rasterize the tab icon.
    if (link.getAttribute("href") !== next) link.setAttribute("href", next);
  });
}

/**
 * Blinks the browser tab title *and favicon* while `count > 0` and the tab is
 * backgrounded, so a blocked session that needs the user is noticeable from
 * another tab without nagging while they're actively looking at the app (the
 * in-app badge already covers that case). Title and icon flip together on a
 * ~1s cadence: `alertTitle` + warning icon, then `baseTitle` + normal icon.
 *
 * While `count > 0` and the tab is *foregrounded*, the title stays normal but
 * the warning icon holds steady — a pinned tab shows only its icon, and a
 * steady color change doesn't nag the way blinking would. Everything restores
 * when the count drops to 0 or the component unmounts.
 *
 * `baseTitle` mirrors what i18n's syncDocument sets (`app.title`), so the
 * cleared state matches the normal title exactly — the two never fight.
 */
export function useTabAlert(count: number, alertTitle: string, baseTitle: string) {
  useEffect(() => {
    if (count <= 0) {
      document.title = baseTitle;
      setAlertIcons(false);
      return;
    }

    let timer: ReturnType<typeof setInterval> | undefined;
    let showAlert = false;

    // Not blinking (tab foregrounded), but still blocked: normal title, steady
    // warning icon.
    const stop = () => {
      if (timer !== undefined) {
        clearInterval(timer);
        timer = undefined;
      }
      document.title = baseTitle;
      setAlertIcons(true);
    };

    const start = () => {
      if (timer !== undefined) return;
      // Show the alert immediately (don't wait a full tick), then toggle the
      // title and icon together.
      showAlert = true;
      document.title = alertTitle;
      setAlertIcons(true);
      timer = setInterval(() => {
        showAlert = !showAlert;
        document.title = showAlert ? alertTitle : baseTitle;
        setAlertIcons(showAlert);
      }, 1000);
    };

    // Only blink while the tab is hidden; a foregrounded tab shows the badge.
    const sync = () => (document.hidden ? start() : stop());
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => {
      document.removeEventListener("visibilitychange", sync);
      // Full restore (unlike stop(), which keeps the warning icon): the next
      // effect run re-asserts the right state if still blocked.
      if (timer !== undefined) clearInterval(timer);
      document.title = baseTitle;
      setAlertIcons(false);
    };
  }, [count, alertTitle, baseTitle]);
}
