import "i18next";
import en from "./locales/en.json";

/**
 * Typed translation keys: `t("bad.key")` fails `tsc --noEmit` (the existing
 * build gate), so a key can't be used without landing in en.json.
 */
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
    resources: { translation: typeof en };
  }
}
