import { diff_match_patch } from "diff-match-patch";

import type { Selector } from "../types/comments";

const CONTEXT_LENGTH = 32;

/** Which strategy resolved a comment's stored selectors to a live Range. */
export type AnchorStrategy = "position" | "quote" | "fuzzy";

/** Result of resolving stored selectors against the live document. */
export interface AnchorResult {
  range: Range;
  strategy: AnchorStrategy;
}

/** Threshold for fuzzy matching: 0.0 = perfect required, 1.0 = anything. */
const FUZZY_THRESHOLD = 0.15;

function getTextContent(container: HTMLElement): string {
  return container.textContent ?? "";
}

/**
 * Convert a DOM Range to an array of selectors for storage.
 * Creates TextQuoteSelector (robust) + TextPositionSelector (fast).
 */
export function rangeToSelectors(range: Range, container: HTMLElement): Selector[] {
  const text = getTextContent(container);
  const exact = range.toString();
  if (!exact) return [];

  const preRange = document.createRange();
  preRange.setStart(container, 0);
  preRange.setEnd(range.startContainer, range.startOffset);
  const start = preRange.toString().length;
  const end = start + exact.length;

  const prefix = text.slice(Math.max(0, start - CONTEXT_LENGTH), start);
  const suffix = text.slice(end, end + CONTEXT_LENGTH);

  return [
    { type: "TextQuoteSelector", exact, prefix, suffix },
    { type: "TextPositionSelector", start, end },
  ];
}

function findTextPosition(
  container: HTMLElement,
  targetOffset: number,
): { node: Text; offset: number } | null {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let currentOffset = 0;

  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const nodeLength = node.textContent?.length ?? 0;
    if (currentOffset + nodeLength >= targetOffset) {
      return { node, offset: targetOffset - currentOffset };
    }
    currentOffset += nodeLength;
  }
  return null;
}

/**
 * Re-anchor stored selectors to a live DOM Range.
 *
 * Resolution order:
 *   1. TextPositionSelector — instant if the offset still points to the
 *      stored quote (validates against TextQuoteSelector.exact when present).
 *   2. TextQuoteSelector — exact substring match. Every occurrence in the
 *      document is scored against the stored prefix/suffix context; the
 *      highest-scoring occurrence wins. The position selector is a tiebreaker
 *      (closest to the original offset), not a search constraint — see N2.
 *   3. Fuzzy match via diff-match-patch — finds an approximate occurrence
 *      near the position hint when the exact substring no longer appears
 *      (typo fix, paragraph split that drops a space, etc.). Marked as
 *      `strategy: 'fuzzy'` so the UI can flag it.
 */
export function selectorsToRange(
  selectors: Selector[],
  container: HTMLElement,
): AnchorResult | null {
  const posSelector = selectors.find(
    (s): s is Extract<Selector, { type: "TextPositionSelector" }> =>
      s.type === "TextPositionSelector",
  );
  const quoteSelector = selectors.find(
    (s): s is Extract<Selector, { type: "TextQuoteSelector" }> => s.type === "TextQuoteSelector",
  );

  if (posSelector) {
    const range = positionToRange(posSelector, container);
    if (range && quoteSelector) {
      if (range.toString() === quoteSelector.exact) {
        return { range, strategy: "position" };
      }
    } else if (range) {
      return { range, strategy: "position" };
    }
  }

  if (quoteSelector) {
    const range = quoteToRange(quoteSelector, container, posSelector?.start);
    if (range) return { range, strategy: "quote" };

    const fuzzy = fuzzyToRange(quoteSelector, container, posSelector?.start);
    if (fuzzy) return { range: fuzzy, strategy: "fuzzy" };
  }

  return null;
}

function positionToRange(
  selector: Extract<Selector, { type: "TextPositionSelector" }>,
  container: HTMLElement,
): Range | null {
  const start = findTextPosition(container, selector.start);
  const end = findTextPosition(container, selector.end);
  if (!start || !end) return null;

  try {
    const range = document.createRange();
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
    return range;
  } catch {
    return null;
  }
}

function quoteToRange(
  selector: Extract<Selector, { type: "TextQuoteSelector" }>,
  container: HTMLElement,
  positionHint?: number,
): Range | null {
  const text = getTextContent(container);
  const { exact, prefix, suffix } = selector;

  // Collect ALL occurrences and score each by context. The position hint is a
  // tiebreaker, not a search constraint — otherwise an occurrence earlier than
  // the hint would be skipped (the bug uncovered by scenario N2).
  let bestIndex = -1;
  let bestScore = -1;
  let bestDistance = Number.POSITIVE_INFINITY;

  let index = text.indexOf(exact);
  while (index !== -1) {
    const score = scoreContext(text, index, exact.length, prefix, suffix);
    const distance = positionHint === undefined ? 0 : Math.abs(index - positionHint);
    if (score > bestScore || (score === bestScore && distance < bestDistance)) {
      bestScore = score;
      bestDistance = distance;
      bestIndex = index;
    }
    index = text.indexOf(exact, index + 1);
  }

  if (bestIndex === -1) return null;

  const start = findTextPosition(container, bestIndex);
  const end = findTextPosition(container, bestIndex + exact.length);
  if (!start || !end) return null;

  try {
    const range = document.createRange();
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
    return range;
  } catch {
    return null;
  }
}

function scoreContext(
  text: string,
  index: number,
  length: number,
  prefix: string,
  suffix: string,
): number {
  let score = 0;
  const actualPrefix = text.slice(Math.max(0, index - prefix.length), index);
  const actualSuffix = text.slice(index + length, index + length + suffix.length);

  for (let i = 0; i < Math.min(prefix.length, actualPrefix.length); i++) {
    if (prefix[prefix.length - 1 - i] === actualPrefix[actualPrefix.length - 1 - i]) {
      score++;
    } else {
      break;
    }
  }

  for (let i = 0; i < Math.min(suffix.length, actualSuffix.length); i++) {
    if (suffix[i] === actualSuffix[i]) {
      score++;
    } else {
      break;
    }
  }

  return score;
}

function fuzzyToRange(
  selector: Extract<Selector, { type: "TextQuoteSelector" }>,
  container: HTMLElement,
  positionHint?: number,
): Range | null {
  const text = getTextContent(container);
  const pattern = selector.exact;
  if (!pattern || !text) return null;

  // diff-match-patch's match_main works in chunks of 32 chars internally; for
  // longer patterns it splits and stitches. Anything longer than the document
  // can't match; bail early.
  if (pattern.length > text.length) return null;

  const dmp = new diff_match_patch();
  dmp.Match_Threshold = FUZZY_THRESHOLD;
  // Allow the match to drift a few quote-lengths from the stored position;
  // beyond that the position hint stops being meaningful.
  dmp.Match_Distance = pattern.length * 4;

  const expectedLoc = positionHint ?? 0;
  // match_main throws "Pattern too long for this browser." when pattern.length
  // exceeds Match_MaxBits (default 32) and no exact substring equals it at the
  // hinted offset. Treat that as "no fuzzy match" instead of letting the
  // exception bubble up and break the caller's anchoring pass.
  let index: number;
  try {
    index = dmp.match_main(text, pattern, expectedLoc);
  } catch {
    return null;
  }
  if (index === -1) return null;

  // diff-match-patch returns a starting index but not a length — it found a
  // region "similar to" the pattern. The actual matched substring may be
  // slightly shorter or longer than `pattern.length`. Anchor to a window of
  // exactly `pattern.length` starting at `index`; this is what Hypothesis
  // does and gives a reasonable visible highlight in practice.
  const start = findTextPosition(container, index);
  const end = findTextPosition(container, Math.min(index + pattern.length, text.length));
  if (!start || !end) return null;

  try {
    const range = document.createRange();
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
    return range;
  } catch {
    return null;
  }
}
