/** Segment by segment, so a `?`, `#` or `%` in a page name stays part of the path. */
export function encodePath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}
