import tseslint from "typescript-eslint";
import i18next from "eslint-plugin-i18next";

// Enforcement gate for the i18n migration: user-facing text must come from
// resource files (src/i18n/locales), never JSX literals. Scope is jsx-only —
// alert()/confirm() args and TS throw messages are covered by convention
// (call-time i18n.t(); see src/i18n/index.ts) since the rule can't see them.
export default tseslint.config({
  files: ["src/**/*.{ts,tsx}"],
  languageOptions: { parser: tseslint.parser },
  plugins: { i18next },
  rules: {
    "i18next/no-literal-string": [
      "error",
      {
        mode: "jsx-only",
        "jsx-attributes": {
          include: ["title", "placeholder", "alt", "aria-label"],
        },
        "should-validate-template": true,
        words: {
          // `exclude` replaces the plugin defaults — the first two entries
          // restore them (punctuation/numbers, SCREAMING_CASE) before our
          // decorative glyphs and untranslated literals (git refs).
          exclude: [
            "[0-9!-/:-@[-`{-~]+",
            "[A-Z_-]+",
            "^[·×…—▾▸◆▪▫⬢⬡⚠🔒⏹💾🗄⬇🔃🔀↑↓⎇⤴\\uFE0F~\\s]+$",
            "HEAD",
          ],
        },
      },
    ],
  },
});
