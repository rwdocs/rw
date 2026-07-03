import { rangeIntersectsNode } from "./ranges";

/** The rendered-diagram boundary. Single-sourced so the comment predicates
 *  below can't drift from each other. */
const DIAGRAM_SELECTOR = "figure.diagram";

/**
 * True when `node` lies inside a rendered diagram figure (`figure.diagram`).
 * Diagrams are inlined SVG whose `<text>` labels are real DOM text nodes; the
 * comment system must treat everything under the figure as non-prose.
 */
export function isInDiagram(node: Node): boolean {
  const el = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  return el != null && el.closest(DIAGRAM_SELECTOR) != null;
}

/**
 * NodeFilter for `createTreeWalker` / `createNodeIterator` that drops any node
 * inside a diagram, so diagram text never enters the commentable text stream.
 */
export const diagramExclusionFilter: NodeFilter = {
  acceptNode(node: Node): number {
    return isInDiagram(node) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
  },
};

/**
 * True when a selection Range starts in, ends in, or spans a diagram. A
 * cross-diagram selection has both endpoints in prose (its
 * commonAncestorContainer is the article), so an endpoint-only `isInDiagram`
 * check misses it — test intersection against every diagram figure instead.
 */
export function rangeTouchesDiagram(range: Range, article: Element): boolean {
  for (const figure of article.querySelectorAll(DIAGRAM_SELECTOR)) {
    if (rangeIntersectsNode(range, figure)) return true;
  }
  return false;
}
