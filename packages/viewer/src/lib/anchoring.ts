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
 *   1. TextPositionSelector — fast path. When a quote selector is also
 *      present, validates that the resolved text equals the stored `exact`
 *      AND that the surrounding context clears the confidence floor (see
 *      isConfidentMatch). Either check failing falls through.
 *   2. TextQuoteSelector — exact substring search with a context-confidence
 *      check. Short quotes (`exact.length` < SHORT_QUOTE_LEN) need strong
 *      agreement on every recorded context side; long quotes need one
 *      strongly-agreeing recorded side. Without confidence we don't anchor
 *      inline — `strategy: 'quote'` is reserved for matches we trust.
 *   3. Fuzzy match via diff-match-patch — runs ONLY when the exact substring
 *      is absent from the document (e.g. typo fix inside the quote,
 *      paragraph split that dropped a space). When the exact text IS present
 *      but failed the confidence gate, fuzzy would just rescue it to the
 *      same wrong place, so we orphan instead. Successful fuzzy matches get
 *      `strategy: 'fuzzy'` (dashed underline + "re-anchored" badge).
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
        const text = getTextContent(container);
        const score = scoreContext(
          text,
          posSelector.start,
          quoteSelector.exact.length,
          quoteSelector.prefix,
          quoteSelector.suffix,
        );
        const occ = { index: posSelector.start, ...score };
        if (
          isConfidentMatch(
            occ,
            quoteSelector.exact.length,
            quoteSelector.prefix,
            quoteSelector.suffix,
          )
        ) {
          return { range, strategy: "position" };
        }
        // Position validated exact but context disagrees — fall through to the
        // quote-search branch, which will reach the same occurrence via
        // quoteBestOccurrence and fail the same gate (orphaning the comment).
      }
    } else if (range) {
      return { range, strategy: "position" };
    }
  }

  if (quoteSelector) {
    const occ = quoteBestOccurrence(quoteSelector, container, posSelector?.start);
    if (occ) {
      if (
        isConfidentMatch(
          occ,
          quoteSelector.exact.length,
          quoteSelector.prefix,
          quoteSelector.suffix,
        )
      ) {
        const range = rangeAtTextOffset(
          container,
          occ.index,
          occ.index + quoteSelector.exact.length,
        );
        if (range) return { range, strategy: "quote" };
      }
      // Exact text is present but failed the confidence gate — the passage
      // exists in a different context. Don't fuzzy-match: a weaker hit would
      // only anchor to the wrong place.
    } else {
      // Exact text is gone — the passage was edited. Try a fuzzy match.
      const fuzzy = fuzzyToRange(quoteSelector, container, posSelector?.start);
      if (fuzzy) return { range: fuzzy, strategy: "fuzzy" };
    }
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

interface ContextScore {
  prefixScore: number;
  suffixScore: number;
}

function scoreContext(
  text: string,
  index: number,
  length: number,
  prefix: string,
  suffix: string,
): ContextScore {
  let prefixScore = 0;
  const actualPrefix = text.slice(Math.max(0, index - prefix.length), index);
  for (let i = 0; i < Math.min(prefix.length, actualPrefix.length); i++) {
    if (prefix[prefix.length - 1 - i] === actualPrefix[actualPrefix.length - 1 - i]) {
      prefixScore++;
    } else {
      break;
    }
  }

  let suffixScore = 0;
  const actualSuffix = text.slice(index + length, index + length + suffix.length);
  for (let i = 0; i < Math.min(suffix.length, actualSuffix.length); i++) {
    if (suffix[i] === actualSuffix[i]) {
      suffixScore++;
    } else {
      break;
    }
  }

  return { prefixScore, suffixScore };
}

// 4 chars rejects accidental single-space agreement (the lone-`-` case
// scores 1 on each side because the bordering spaces happen to match)
// while accepting a clear word boundary like "foo " (score 4).
const MIN_CONTEXT_PER_SIDE = 4;

// At ~8 chars an exact text carries enough identifying signal on its own
// that one strong context side is sufficient; below that, the exact is
// too common to trust without agreement on BOTH sides (`-`, `,`, `TODO`).
const SHORT_QUOTE_LEN = 8;

interface QuoteOccurrence {
  index: number;
  prefixScore: number;
  suffixScore: number;
}

/**
 * Search the live text for the best occurrence of `exact`. Returns the
 * occurrence with the highest sum of per-side context scores (with
 * position-hint distance as a tiebreaker) along with its per-side context
 * scores, or null if `exact` is not present.
 */
function quoteBestOccurrence(
  selector: Extract<Selector, { type: "TextQuoteSelector" }>,
  container: HTMLElement,
  positionHint?: number,
): QuoteOccurrence | null {
  const text = getTextContent(container);
  const { exact, prefix, suffix } = selector;
  // Empty exact would spin the loop below forever — indexOf("", n) returns n, never -1.
  if (!exact) return null;

  let best: QuoteOccurrence | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;

  let index = text.indexOf(exact);
  while (index !== -1) {
    const { prefixScore, suffixScore } = scoreContext(text, index, exact.length, prefix, suffix);
    const distance = positionHint === undefined ? 0 : Math.abs(index - positionHint);
    const currentSum = prefixScore + suffixScore;
    const bestSum = best ? best.prefixScore + best.suffixScore : -1;
    if (currentSum > bestSum || (currentSum === bestSum && distance < bestDistance)) {
      best = { index, prefixScore, suffixScore };
      bestDistance = distance;
    }
    index = text.indexOf(exact, index + 1);
  }

  return best;
}

/**
 * Decide whether a quote occurrence has enough surrounding-context agreement
 * to be trusted as the original passage.
 *
 *   - Short quotes (`exact.length` < SHORT_QUOTE_LEN): every RECORDED side
 *     must agree to its achievable threshold. A side we didn't record is
 *     vacuously ok — but a side we DID record cannot be brushed off.
 *   - Long quotes (`exact.length` >= SHORT_QUOTE_LEN): at least one RECORDED
 *     side must strongly agree. An empty side carries no evidence, so it
 *     cannot be "the strong side."
 *   - Both sides empty: accept unconditionally (no context to judge against).
 *
 * The achievable threshold for each side is
 * `min(MIN_CONTEXT_PER_SIDE, recordedSide.length)` — a 2-char stored prefix
 * (boundary selection, tiny article) saturates at 2 and shouldn't be held to
 * a 4-char floor it can never reach.
 */
function isConfidentMatch(
  occ: QuoteOccurrence,
  exactLen: number,
  prefix: string,
  suffix: string,
): boolean {
  const havePrefix = prefix.length > 0;
  const haveSuffix = suffix.length > 0;
  if (!havePrefix && !haveSuffix) return true;

  const prefixThreshold = Math.min(MIN_CONTEXT_PER_SIDE, prefix.length);
  const suffixThreshold = Math.min(MIN_CONTEXT_PER_SIDE, suffix.length);

  const prefixGood = havePrefix && occ.prefixScore >= prefixThreshold;
  const suffixGood = haveSuffix && occ.suffixScore >= suffixThreshold;

  if (exactLen < SHORT_QUOTE_LEN) {
    const prefixOk = !havePrefix || prefixGood;
    const suffixOk = !haveSuffix || suffixGood;
    return prefixOk && suffixOk;
  }
  return prefixGood || suffixGood;
}

function rangeAtTextOffset(container: HTMLElement, start: number, end: number): Range | null {
  const startPos = findTextPosition(container, start);
  const endPos = findTextPosition(container, end);
  if (!startPos || !endPos) return null;

  try {
    const range = document.createRange();
    range.setStart(startPos.node, startPos.offset);
    range.setEnd(endPos.node, endPos.offset);
    return range;
  } catch {
    return null;
  }
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
