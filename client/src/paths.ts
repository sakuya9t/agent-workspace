/**
 * Shorten a long absolute path to a ".../parent/leaf" display form, keeping
 * the separator the path uses (a Windows host reports `C:\Users\…`).
 */
export function shortPath(p: string): string {
  const sep = !p.includes("/") && p.includes("\\") ? "\\" : "/";
  const parts = p.split(sep);
  if (parts.length <= 3) return p;
  return `...${sep}` + parts.slice(-2).join(sep);
}
