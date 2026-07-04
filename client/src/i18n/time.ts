import i18n from "./index";

/**
 * Shared relative-time formatting — replaces the three near-duplicate
 * hand-rolled formatters that lived in SessionList, RightPanel and UsageModal.
 * Strings resolve through i18n at call time; the keys carry `count`, so a
 * future locale can add `_one`/`_other` plural variants without code changes.
 */
export function relTime(ms: number): string {
  const d = Date.now() - ms;
  if (d < 5000) return i18n.t("time.justNow");
  if (d < 60000) return i18n.t("time.secondsAgo", { count: Math.floor(d / 1000) });
  if (d < 3600000) return i18n.t("time.minutesAgo", { count: Math.floor(d / 60000) });
  if (d < 86400000) return i18n.t("time.hoursAgo", { count: Math.floor(d / 3600000) });
  if (d < 2592000000) return i18n.t("time.daysAgo", { count: Math.floor(d / 86400000) });
  return i18n.t("time.monthsAgo", { count: Math.floor(d / 2592000000) });
}

/** Future countdown for rate-limit reset times ("in 2h 15m"). */
export function fmtReset(unixSecs: number): string {
  const diff = unixSecs * 1000 - Date.now();
  if (diff <= 0) return i18n.t("time.resetNow");
  const mins = Math.round(diff / 60000);
  if (mins < 60) return i18n.t("time.inMinutes", { m: mins });
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  if (hours < 24) {
    return rem
      ? i18n.t("time.inHoursMinutes", { h: hours, m: rem })
      : i18n.t("time.inHours", { h: hours });
  }
  return i18n.t("time.inDaysHours", { d: Math.floor(hours / 24), h: hours % 24 });
}
