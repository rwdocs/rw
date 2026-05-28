import type { AnchorStrategy } from "$lib/anchoring";

export interface WrapAttrs {
  commentId: string;
  strategy: AnchorStrategy;
}

/**
 * Wrap a Range's text in one or more `<rw-annotation>` elements. Splits text
 * nodes at the range boundaries. A range that crosses tag boundaries (e.g.
 * `Hello <em>world</em>` covering `lo wor`) produces multiple wrappers — one
 * per contiguous-sibling text-node span — all carrying the same `data-comment-id`.
 *
 * Returns the wrappers created (in document order). Empty array if the range
 * collapses or contains no non-whitespace text.
 */
export function wrapRange(range: Range, attrs: WrapAttrs): HTMLElement[] {
  void range;
  void attrs;
  return [];
}

/**
 * Remove every `<rw-annotation>` descendant of `container`, restoring the
 * original text-node structure. Idempotent.
 */
export function unwrapAll(container: HTMLElement): void {
  void container;
}
