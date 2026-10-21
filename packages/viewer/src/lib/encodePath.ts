/** The server's `PATH` set (`rw-renderer`'s `link.rs`), so nav and content hrefs spell a page alike. */
export function encodePath(path: string): string {
  return path.replace(/[\p{Cc}\P{ASCII} "#%<>?\\`{}]+/gu, encodeURIComponent);
}
