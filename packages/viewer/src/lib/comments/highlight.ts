import type { AnchorStrategy } from "$lib/anchoring";

export interface WrapAttrs {
  commentId: string;
  strategy: AnchorStrategy;
}

const WHITESPACE_ONLY = /^\s*$/;

/**
 * Escape a comment id for use in a CSS attribute selector, with a fallback for
 * environments lacking `CSS.escape` (older runtimes, some test DOMs). Shared by
 * every site that looks up an annotation/card by `data-comment-id` /
 * `data-thread-id`.
 */
export function escapeId(id: string): string {
  return typeof CSS !== "undefined" && CSS.escape ? CSS.escape(id) : id;
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
  const textNodes = wholeTextNodesInRange(range);
  const spans = groupContiguousSiblings(textNodes);
  const wrappers: HTMLElement[] = [];
  for (const span of spans) {
    if (span.every((n) => WHITESPACE_ONLY.test(n.data))) continue;
    const wrapper = document.createElement("rw-annotation");
    wrapper.setAttribute("data-comment-id", attrs.commentId);
    wrapper.setAttribute("data-strategy", attrs.strategy);
    const parent = span[0].parentNode;
    if (!parent) continue;
    parent.insertBefore(wrapper, span[0]);
    for (const node of span) {
      wrapper.appendChild(node);
    }
    wrappers.push(wrapper);
  }
  return wrappers;
}

/**
 * Collect every text node fully inside `range`, splitting the start and end
 * text nodes at the range boundaries if needed. Adapted from Hypothesis
 * (`src/annotator/highlighter.ts` — `wholeTextNodesInRange`).
 */
function wholeTextNodesInRange(range: Range): Text[] {
  if (range.collapsed) return [];

  let root: Node | null = range.commonAncestorContainer;
  if (root && root.nodeType !== Node.ELEMENT_NODE) {
    root = root.parentElement;
  }
  if (!root) return [];

  const out: Text[] = [];
  const iter = root.ownerDocument!.createNodeIterator(root, NodeFilter.SHOW_TEXT);
  let node: Node | null;
  while ((node = iter.nextNode())) {
    const text = node as Text;

    // Split off the prefix of the start node so the returned node starts at
    // the range's start. splitText mutates the original text node to hold the
    // prefix and returns a new node holding the tail; per DOM spec, any range
    // boundary at offset >= split offset moves to the new node. So after the
    // split, range.startContainer points at the tail — we'll pick it up on
    // the iterator's next pass. We must check the splits *before*
    // `nodeInRange`, which requires full containment and would reject a
    // text node that's only partially in the range.
    if (text === range.startContainer && range.startOffset > 0) {
      text.splitText(range.startOffset);
      continue;
    }
    // Split off the suffix of the end node so the kept head ends at the
    // range's end. After this split, `text` (the head) is fully inside the
    // range — `nodeInRange` below will accept it.
    if (text === range.endContainer && range.endOffset < text.data.length) {
      text.splitText(range.endOffset);
    }
    if (!nodeInRange(range, text)) continue;
    out.push(text);
  }
  return out;
}

function nodeInRange(range: Range, node: Node): boolean {
  const nodeRange = node.ownerDocument!.createRange();
  nodeRange.selectNodeContents(node);
  const startsAfter = range.compareBoundaryPoints(Range.START_TO_START, nodeRange) <= 0;
  const endsBefore = range.compareBoundaryPoints(Range.END_TO_END, nodeRange) >= 0;
  nodeRange.detach();
  return startsAfter && endsBefore;
}

function groupContiguousSiblings(nodes: Text[]): Text[][] {
  const spans: Text[][] = [];
  let prev: Node | null = null;
  let current: Text[] | null = null;
  for (const node of nodes) {
    if (prev && prev.nextSibling === node) {
      current!.push(node);
    } else {
      current = [node];
      spans.push(current);
    }
    prev = node;
  }
  return spans;
}

/**
 * Remove every `<rw-annotation>` descendant of `container`, restoring the
 * original text-node structure. Idempotent. No-op when no wrappers exist.
 */
export function unwrapAll(container: HTMLElement): void {
  // querySelectorAll returns a static NodeList — snapshotting once is safe
  // because unwrapping moves a wrapper's children up to its parent without
  // invalidating other entries in the snapshot. Iterate in document order;
  // nested wrappers are unwrapped from outside-in, which is correct: the
  // outer's children (including the inner wrapper) get moved up first, then
  // the inner gets unwrapped in turn.
  const wrappers = container.querySelectorAll("rw-annotation");
  if (wrappers.length === 0) return;
  for (const wrapper of wrappers) {
    const parent = wrapper.parentNode;
    if (!parent) continue;
    while (wrapper.firstChild) {
      parent.insertBefore(wrapper.firstChild, wrapper);
    }
    parent.removeChild(wrapper);
  }
  container.normalize();
}
