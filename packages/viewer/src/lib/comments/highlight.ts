import type { AnchorStrategy } from "$lib/anchoring";

export interface WrapAttrs {
  commentId: string;
  strategy: AnchorStrategy;
}

const WHITESPACE_ONLY = /^\s*$/;

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
  // node is in range when its start is at-or-after the range start AND
  // its end is at-or-before the range end.
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
