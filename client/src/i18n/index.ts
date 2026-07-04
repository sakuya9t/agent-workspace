import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";

/**
 * i18n bootstrap. Resources are bundled and init is synchronous, so `t()` is
 * valid from the moment this module is imported — `main.tsx` imports it before
 * anything that could render or throw a translated string.
 *
 * Adding a locale later: drop a `locales/<lng>.json` (mirror en.json; enforce
 * parity with `theNew satisfies typeof en`), list it in SUPPORTED and
 * `resources`, and add a picker that calls `i18next.changeLanguage`.
 */
const LANG_KEY = "asm.lang";
export const SUPPORTED = ["en"] as const;

/** `?lang=` (ephemeral, for testing) → persisted asm.lang → en. */
function detectLanguage(): string {
  const param = new URLSearchParams(location.search).get("lang");
  if (param && (SUPPORTED as readonly string[]).includes(param)) return param;
  try {
    const stored = localStorage.getItem(LANG_KEY);
    if (stored && (SUPPORTED as readonly string[]).includes(stored)) return stored;
  } catch {
    // storage may be unavailable (privacy mode); fall through to default
  }
  return "en";
}

i18next.use(initReactI18next).init({
  resources: { en: { translation: en } },
  lng: detectLanguage(),
  fallbackLng: "en",
  // React escapes on render, and confirm()/alert() strings must not contain
  // HTML entities — never let i18next escape interpolated values.
  interpolation: { escapeValue: false },
  react: { useSuspense: false },
  initAsync: false,
});

function syncDocument() {
  document.documentElement.lang = i18next.language;
  document.title = i18next.t("app.title");
}
i18next.on("languageChanged", syncDocument);
syncDocument();

export default i18next;
