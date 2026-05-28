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
  // Iterate while at least one wrapper remains. We can't snapshot the
  // NodeList up front because replacing a parent wrapper detaches its
  // children (which may include nested wrappers) from the original
  // querySelectorAll() result's parent, but querying fresh each loop
  // is simpler and correct.
  let wrapper = container.querySelector("rw-annotation");
  while (wrapper) {
    const parent = wrapper.parentNode;
    if (!parent) break;
    while (wrapper.firstChild) {
      parent.insertBefore(wrapper.firstChild, wrapper);
    }
    parent.removeChild(wrapper);
    wrapper = container.querySelector("rw-annotation");
  }
  container.normalize();
}
