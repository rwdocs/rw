/**
 * Keeps the reader's place across a live reload. The reload replaces every node
 * of the article, which defeats the browser's own scroll anchoring, so the text
 * at the top of the view is recorded the way a comment's passage is and found
 * again in the new article.
 */
import { on } from "svelte/events";
import type { Selector } from "../types/comments";
import { buildTextIndex, rangeToSelectors, selectorsToRange } from "./anchoring";
import { scrollerOf } from "./ui/scroller";

export interface ReadingAnchor {
  selectors: Selector[];
  /** Where the text's first line sat, in viewport coordinates. */
  top: number;
}

const QUOTE_LENGTH = 30;
const REALIGN_MS = 1000;

/** At the very top, text added above the title is the reader's to see. */
function isScrolledToTop(article: HTMLElement): boolean {
  const scroller = scrollerOf(article);
  return (scroller instanceof Window ? scroller.scrollY : scroller.scrollTop) === 0;
}

/** Skips zero-width rects: at a line end, a range can report one on the line before. */
function lineTop(range: Range): number {
  const rect = [...range.getClientRects()].find((r) => r.width > 0);
  return (rect ?? range.getBoundingClientRect()).top;
}

/** The article's top-left in view, below anything covering it: the sticky mobile
 *  header, a comment popover. */
function pointBelowOverlays(article: HTMLElement): { x: number; y: number } {
  const x = article.getBoundingClientRect().left + 1;
  let y = 1;
  let hit = document.elementFromPoint(x, y);
  while (hit && !article.contains(hit)) {
    y = hit.getBoundingClientRect().bottom + 1;
    hit = y < innerHeight ? document.elementFromPoint(x, y) : null;
  }
  return { x, y };
}

/** `caretRangeFromPoint` covers browsers without `caretPositionFromPoint`. */
function caretAt(x: number, y: number): { node: Node; offset: number } | null {
  const position = document.caretPositionFromPoint?.(x, y);
  if (position) return { node: position.offsetNode, offset: position.offset };
  const range = document.caretRangeFromPoint?.(x, y);
  return range ? { node: range.startContainer, offset: range.startOffset } : null;
}

/** Up to `length` characters of the article's prose from (node, offset). A point
 *  on an image or diagram yields the text after it. */
export function textRangeFrom(
  article: HTMLElement,
  node: Node,
  offset: number,
  length = QUOTE_LENGTH,
): Range | null {
  const index = buildTextIndex(article);
  const start = index.offsetOf(node, offset) ?? 0;
  const from = index.locate(start, "forward");
  const to = index.locate(Math.min(start + length, index.text.length), "backward");
  if (!from || !to) return null;
  const range = document.createRange();
  range.setStart(from.node, from.offset);
  range.setEnd(to.node, to.offset);
  return range;
}

export function captureReadingAnchor(article: HTMLElement): ReadingAnchor | null {
  if (isScrolledToTop(article)) return null;
  const { x, y } = pointBelowOverlays(article);
  const caret = caretAt(x, y);
  // Off the article, such as above it, its first text stands in.
  const point = caret && article.contains(caret.node) ? caret : { node: article, offset: 0 };
  const range = textRangeFrom(article, point.node, point.offset);
  const selectors = range ? rangeToSelectors(range, article) : [];
  return range && selectors.length > 0 ? { selectors, top: lineTop(range) } : null;
}

/** Exact matches only: a fuzzy one can be a look-alike elsewhere, such as the next
 *  section's opening after the reader's heading was removed. */
export function findReadingAnchor(article: HTMLElement, anchor: ReadingAnchor): Range | null {
  const found = selectorsToRange(anchor.selectors, article);
  return found && found.strategy !== "fuzzy" ? found.range : null;
}

/** Scrolls the anchored text back to where it was; returns the cleanup. */
export function restoreReadingAnchor(article: HTMLElement, anchor: ReadingAnchor): () => void {
  const range = findReadingAnchor(article, anchor);
  if (!range) return () => {};
  const scroller = scrollerOf(article);
  const drift = () => lineTop(range) - anchor.top;
  const realign = () => scroller.scrollBy(0, drift());
  realign();

  // Images that finish loading after the swap move the text again, which browsers
  // don't reliably anchor. A scroll that moves the text is the reader's.
  const offs = [
    on(article, "load", realign, { capture: true }),
    on(window, "scroll", () => Math.abs(drift()) > 1 && stop(), { capture: true }),
  ];
  const timer = setTimeout(stop, REALIGN_MS);
  function stop(): void {
    clearTimeout(timer);
    for (const off of offs) off();
  }
  return stop;
}
