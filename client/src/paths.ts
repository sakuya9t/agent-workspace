/** Shorten a long absolute path to a ".../parent/leaf" display form. */
export function shortPath(p: string): string {
  const parts = p.split("/");
  if (parts.length <= 3) return p;
  return ".../" + parts.slice(-2).join("/");
}
