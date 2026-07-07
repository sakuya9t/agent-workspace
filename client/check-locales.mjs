#!/usr/bin/env node
// Locale parity check — every file in src/i18n/locales must stay in sync with
// en.json (the source of truth). Runs in `npm run lint` and the build gate.
//
// Reported per locale file:
//   - missing keys   (in en.json, absent in the locale)
//   - orphan keys    (in the locale, unknown to en.json)
//   - empty values   (would silently render nothing)
//   - unknown {{slots}} or <tags> (not present in the en value — typos render
//     literally in the UI; `count` is always allowed, it's the plural input)
//
// Plural variants (_one/_other/…) are compared as one key family: a locale
// legitimately needs different plural forms than English (zh has only _other).

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const LOCALES_DIR = join(dirname(fileURLToPath(import.meta.url)), "src/i18n/locales");
const REFERENCE = "en.json";
const PLURAL_SUFFIX = /_(zero|one|two|few|many|other)$/;

function flatten(obj, prefix = "", out = new Map()) {
  for (const [k, v] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object") flatten(v, path, out);
    else out.set(path, String(v));
  }
  return out;
}

const familyOf = (key) => key.replace(PLURAL_SUFFIX, "");
const slotsOf = (value) => new Set([...value.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]));
const tagsOf = (value) => new Set([...value.matchAll(/<\/?(\w+)/g)].map((m) => m[1]));

/** family -> union of interpolation slots / Trans tags across its variants */
function families(flat) {
  const map = new Map();
  for (const [key, value] of flat) {
    const fam = familyOf(key);
    const f = map.get(fam) ?? { slots: new Set(), tags: new Set() };
    for (const s of slotsOf(value)) f.slots.add(s);
    for (const t of tagsOf(value)) f.tags.add(t);
    map.set(fam, f);
  }
  return map;
}

const en = flatten(JSON.parse(readFileSync(join(LOCALES_DIR, REFERENCE), "utf8")));
const enFams = families(en);

const localeFiles = readdirSync(LOCALES_DIR)
  .filter((f) => f.endsWith(".json") && f !== REFERENCE)
  .sort();

let errors = 0;
const report = (file, msg) => {
  console.error(`${file}: ${msg}`);
  errors++;
};

for (const file of localeFiles) {
  let flat;
  try {
    flat = flatten(JSON.parse(readFileSync(join(LOCALES_DIR, file), "utf8")));
  } catch (e) {
    report(file, `unreadable JSON: ${e.message}`);
    continue;
  }
  const fams = families(flat);

  for (const fam of enFams.keys()) {
    if (!fams.has(fam)) report(file, `missing key: ${fam}`);
  }
  for (const [fam, { slots, tags }] of fams) {
    const ref = enFams.get(fam);
    if (!ref) {
      report(file, `orphan key (not in ${REFERENCE}): ${fam}`);
      continue;
    }
    for (const s of slots) {
      if (s !== "count" && !ref.slots.has(s)) report(file, `unknown slot {{${s}}} in ${fam}`);
    }
    for (const t of tags) {
      if (!ref.tags.has(t)) report(file, `unknown <${t}> tag in ${fam}`);
    }
  }
  for (const [key, value] of flat) {
    if (value.trim() === "") report(file, `empty value: ${key}`);
  }
}

if (errors) {
  console.error(
    `\ncheck-locales: ${errors} problem(s). ${REFERENCE} is the source of truth; see docs/i18n.md.`,
  );
  process.exit(1);
}
console.log(
  localeFiles.length
    ? `check-locales: ${localeFiles.join(", ")} in sync with ${REFERENCE}`
    : `check-locales: only ${REFERENCE} present — nothing to compare`,
);
