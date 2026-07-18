import approxSearch from "approx-string-match";

import type { Selector } from "../types/comments";
import { diagramExclusionFilter } from "./comments/diagram";

const CONTEXT_LENGTH = 32;

/** Which strategy resolved a comment's stored selectors to a live Range. */
export type AnchorStrategy = "position" | "quote" | "fuzzy";

/** Result of resolving stored selectors against the live document. */
export interface AnchorResult {
  range: Range;
  strategy: AnchorStrategy;
}

/**
 * Maximum edit distance for a fuzzy match, as a fraction of the quote length:
 * `maxErrors = ceil(exact.length * FUZZY_MAX_ERROR_FRACTION)`. At 0.3 a quote
 * re-anchors after up to ~30% of its characters change. Realistic edits to a
 * still-present passage sit well under this (empirically ~0.2), while heavier
 * rewrites fall through to the page timeline. Precision is enforced by the
 * context/uniqueness gate (isConfidentMatch), not by this budget — a sweep of
 * 0.3/0.4/0.5 produced identical anchoring outcomes, so 0.3 is chosen as the
 * tightest value with no recall loss and the fewest spurious candidates.
 */
const FUZZY_MAX_ERROR_FRACTION = 0.3;

export interface TextIndex {
  /** Full concatenated text content of the container, in document order. */
  text: string;
  /** Map a character offset in `text` to a live text node + local offset. */
  locate(offset: number): { node: Text; offset: number } | null;
  /** Map a DOM boundary point (node, offset) to its offset in `text`. */
  offsetOf(node: Node, nodeOffset: number): number | null;
}

/**
 * Walk the container's text nodes ONCE, recording each node and its cumulative
 * start offset, and concatenate the full text. `locate` then binary-searches the
 * cumulative offsets instead of re-walking per call. A pass that anchors many
 * comments builds this once and reuses it for every comment.
 */
export function buildTextIndex(container: HTMLElement): TextIndex {
  const nodes: Text[] = [];
  const starts: number[] = []; // starts[i] = cumulative text offset where nodes[i] begins
  let text = "";
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, diagramExclusionFilter);
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    starts.push(text.length);
    nodes.push(node);
    text += node.data;
  }

  function locate(offset: number): { node: Text; offset: number } | null {
    if (offset < 0 || offset > text.length || nodes.length === 0) return null;
    // Find the last node whose start is strictly less than offset (binary search).
    // At a node boundary (starts[i] === offset) this returns the earlier node
    // with local offset === node.length rather than the next node at offset 0 —
    // both are valid Range endpoints, but anchoring to the earlier node's end
    // avoids crossing into a following sibling span and producing an unexpected
    // boundary.
    let lo = 0;
    let hi = nodes.length - 1;
    let idx = 0;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (starts[mid] < offset) {
        idx = mid;
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return { node: nodes[idx], offset: offset - starts[idx] };
  }

  // Inverse of `locate`: walk the indexed (diagram-excluded) nodes in document
  // order, summing lengths of nodes that fall before the boundary point, and a
  // partial length for the node the boundary falls inside. Diagram text is not
  // in `nodes`, so a boundary sitting after a diagram gets the filtered offset.
  function offsetOf(node: Node, nodeOffset: number): number | null {
    const point = document.createRange();
    try {
      point.setStart(node, nodeOffset);
      point.setEnd(node, nodeOffset);
    } catch {
      return null;
    }
    let total = 0;
    for (let i = 0; i < nodes.length; i++) {
      const t = nodes[i];
      const nodeRange = document.createRange();
      nodeRange.selectNodeContents(t);
      // Whole node ends at or before the point -> count all of it.
      if (nodeRange.compareBoundaryPoints(Range.START_TO_END, point) <= 0) {
        total += t.data.length;
        continue;
      }
      // Whole node starts at or after the point -> we're done.
      if (nodeRange.compareBoundaryPoints(Range.START_TO_START, point) >= 0) {
        break;
      }
      // The point lies inside this text node; its container is this node, so
      // nodeOffset is the local character offset.
      total += nodeOffset;
      break;
    }
    return total;
  }

  return { text, locate, offsetOf };
}

/**
 * Convert a DOM Range to an array of selectors for storage.
 * Creates TextQuoteSelector (robust) + TextPositionSelector (fast).
 *
 * Offsets, quote, and context are all computed against the diagram-excluded
 * text stream (via buildTextIndex), so they line up with resolution and a
 * diagram sitting above the selection can't skew them. Returns [] when the
 * selection projects to no prose (e.g. it lies entirely inside a diagram).
 */
export function rangeToSelectors(range: Range, container: HTMLElement): Selector[] {
  const index = buildTextIndex(container);
  const start = index.offsetOf(range.startContainer, range.startOffset);
  const end = index.offsetOf(range.endContainer, range.endOffset);
  if (start === null || end === null || start >= end) return [];

  const exact = index.text.slice(start, end);
  if (!exact) return [];

  const prefix = index.text.slice(Math.max(0, start - CONTEXT_LENGTH), start);
  const suffix = index.text.slice(end, end + CONTEXT_LENGTH);

  return [
    { type: "TextQuoteSelector", exact, prefix, suffix },
    { type: "TextPositionSelector", start, end },
  ];
}

/**
 * Re-anchor stored selectors to a live DOM Range using a pre-built TextIndex.
 *
 * Resolution order:
 *   1. TextPositionSelector — fast path. When a quote selector is also
 *      present, validates that the resolved text equals the stored `exact`
 *      AND that the surrounding context clears the confidence floor (see
 *      isConfidentMatch). Either check failing falls through.
 *   2. TextQuoteSelector — exact substring search with a context-confidence
 *      check. A non-unique short quote (`exact.length` < SHORT_QUOTE_LEN)
 *      needs strong agreement on every recorded context side; a unique short
 *      quote or any long quote needs one strongly-agreeing recorded side.
 *      Without confidence we don't anchor inline — `strategy: 'quote'` is
 *      reserved for matches we trust.
 *   3. Fuzzy match via approximate (Myers) string search — runs ONLY when the
 *      exact substring is absent from the document (e.g. typo fix inside the
 *      quote, a word substitution, a paragraph split that dropped a space).
 *      When the exact text IS present but failed the confidence gate, fuzzy
 *      would just rescue it to the same wrong place, so we orphan instead.
 *      Successful fuzzy matches get `strategy: 'fuzzy'` (dashed underline +
 *      "fuzzy" badge).
 */
export function selectorsToRangeIn(selectors: Selector[], index: TextIndex): AnchorResult | null {
  const posSelector = selectors.find(
    (s): s is Extract<Selector, { type: "TextPositionSelector" }> =>
      s.type === "TextPositionSelector",
  );
  const quoteSelector = selectors.find(
    (s): s is Extract<Selector, { type: "TextQuoteSelector" }> => s.type === "TextQuoteSelector",
  );

  if (posSelector) {
    const range = positionToRange(posSelector, index);
    if (range && quoteSelector) {
      if (range.toString() === quoteSelector.exact) {
        const score = scoreContext(
          index.text,
          posSelector.start,
          quoteSelector.exact.length,
          quoteSelector.prefix,
          quoteSelector.suffix,
        );
        // The position path never searched the document, so it can't claim
        // uniqueness — isUnique:false keeps a short exact on the strict
        // both-sides gate (long exacts are unaffected either way).
        const occ = { index: posSelector.start, isUnique: false, ...score };
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
        // quote-search branch, which re-evaluates this occurrence with
        // uniqueness in play: a unique short exact can still anchor there on a
        // single strong side; otherwise the comment orphans.
      }
    } else if (range) {
      return { range, strategy: "position" };
    }
  }

  if (quoteSelector) {
    const occ = quoteBestOccurrence(quoteSelector, index, posSelector?.start);
    if (occ) {
      if (
        isConfidentMatch(
          occ,
          quoteSelector.exact.length,
          quoteSelector.prefix,
          quoteSelector.suffix,
        )
      ) {
        const range = rangeAtTextOffset(index, occ.index, occ.index + quoteSelector.exact.length);
        if (range) return { range, strategy: "quote" };
      }
      // Exact text is present but failed the confidence gate — the passage
      // exists in a different context. Don't fuzzy-match: a weaker hit would
      // only anchor to the wrong place.
    } else {
      // Exact text is gone — the passage was edited. Try a fuzzy match.
      const fuzzy = fuzzyToRange(quoteSelector, index, posSelector?.start);
      if (fuzzy) return { range: fuzzy, strategy: "fuzzy" };
    }
  }

  return null;
}

/**
 * Re-anchor stored selectors to a live DOM Range.
 *
 * Builds a TextIndex from `container` and delegates to `selectorsToRangeIn`.
 * When anchoring many comments against the same container, prefer building
 * one TextIndex with `buildTextIndex` and calling `selectorsToRangeIn` directly.
 */
export function selectorsToRange(
  selectors: Selector[],
  container: HTMLElement,
): AnchorResult | null {
  return selectorsToRangeIn(selectors, buildTextIndex(container));
}

function positionToRange(
  selector: Extract<Selector, { type: "TextPositionSelector" }>,
  index: TextIndex,
): Range | null {
  const start = index.locate(selector.start);
  const end = index.locate(selector.end);
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
// that one strong context side is sufficient; below that, a NON-unique exact
// is too common to trust without agreement on BOTH sides (`-`, `,`, `TODO`).
// A unique short exact has nothing to disambiguate against, so it too needs
// only one strong side (see isConfidentMatch).
const SHORT_QUOTE_LEN = 8;

interface QuoteOccurrence {
  index: number;
  prefixScore: number;
  suffixScore: number;
  /** True when `exact` occurs exactly once in the live document. */
  isUnique: boolean;
}

/**
 * Search the live text for the best occurrence of `exact`. Returns the
 * occurrence with the highest sum of per-side context scores (with
 * position-hint distance as a tiebreaker) along with its per-side context
 * scores, or null if `exact` is not present.
 */
function quoteBestOccurrence(
  selector: Extract<Selector, { type: "TextQuoteSelector" }>,
  index: TextIndex,
  positionHint?: number,
): QuoteOccurrence | null {
  const { exact, prefix, suffix } = selector;
  // Empty exact would spin the loop below forever — indexOf("", n) returns n, never -1.
  if (!exact) return null;

  let best: QuoteOccurrence | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  let count = 0;

  let i = index.text.indexOf(exact);
  while (i !== -1) {
    count++;
    const { prefixScore, suffixScore } = scoreContext(index.text, i, exact.length, prefix, suffix);
    const distance = positionHint === undefined ? 0 : Math.abs(i - positionHint);
    const currentSum = prefixScore + suffixScore;
    const bestSum = best ? best.prefixScore + best.suffixScore : -1;
    if (currentSum > bestSum || (currentSum === bestSum && distance < bestDistance)) {
      best = { index: i, prefixScore, suffixScore, isUnique: false };
      bestDistance = distance;
    }
    i = index.text.indexOf(exact, i + 1);
  }

  if (best) best.isUnique = count === 1;
  return best;
}

/**
 * Decide whether a quote occurrence has enough surrounding-context agreement
 * to be trusted as the original passage.
 *
 *   - Short quotes (`exact.length` < SHORT_QUOTE_LEN): the both-sides rule
 *     exists only to disambiguate a short exact among multiple occurrences.
 *     When the exact is UNIQUE (`occ.isUnique`) there is nothing to disambiguate,
 *     so one strongly-agreeing RECORDED side is enough — this keeps a comment
 *     on e.g. a short heading anchored when the text next to it is edited.
 *     A NON-unique short exact still needs every RECORDED side to agree (a
 *     side we didn't record is vacuously ok; a side we DID record cannot be
 *     brushed off).
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
    // A unique short exact has nothing to disambiguate against, so one strong
    // recorded side is sufficient (same as the long-quote rule below).
    if (occ.isUnique) return prefixGood || suffixGood;
    const prefixOk = !havePrefix || prefixGood;
    const suffixOk = !haveSuffix || suffixGood;
    return prefixOk && suffixOk;
  }
  return prefixGood || suffixGood;
}

function rangeAtTextOffset(index: TextIndex, start: number, end: number): Range | null {
  const startPos = index.locate(start);
  const endPos = index.locate(end);
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
  index: TextIndex,
  positionHint?: number,
): Range | null {
  const { exact, prefix, suffix } = selector;
  if (!exact || !index.text) return null;

  // Myers bit-vector approximate match — no pattern-length limit, so
  // sentence-length quotes (the common case) can be fuzzy-matched at all.
  // NOTE: approx-string-match returns only the region(s) with the *lowest*
  // error count it finds, not every region within the budget. Consequence: if
  // another passage on the page is strictly more similar to the stored quote
  // than the lightly-edited original, it shadows the original (which is then
  // never returned). The context gate below is what stops such a coincidence
  // from anchoring to the wrong place; we accept the residual recall loss. Do
  // NOT re-introduce a position-distance penalty to recover it: that reinstates
  // a stale-hint barrier where a comment whose page was edited above it can no
  // longer re-anchor.
  const maxErrors = Math.ceil(exact.length * FUZZY_MAX_ERROR_FRACTION);
  const matches = approxSearch(index.text, exact, maxErrors);
  if (matches.length === 0) return null;

  // Rank the returned candidates: fewer edits first (kept for intent and
  // forward-compat — with the current matcher this clause never fires, since
  // approxSearch already returns only lowest-error matches); then stronger
  // surrounding-context agreement; then proximity to the stored position hint.
  // The hint is a tiebreaker ONLY — no hard distance penalty — so a comment
  // whose page was edited above it (a drifted hint) is not blocked from
  // re-anchoring. This parallels quoteBestOccurrence's context + distance
  // tiebreak, including neutralizing the hint when it is absent.
  const best = matches
    .map((m) => ({
      match: m,
      ...scoreContext(index.text, m.start, m.end - m.start, prefix, suffix),
    }))
    .sort((a, b) => {
      if (a.match.errors !== b.match.errors) return a.match.errors - b.match.errors;
      const context = b.prefixScore + b.suffixScore - (a.prefixScore + a.suffixScore);
      if (context !== 0) return context;
      if (positionHint === undefined) return 0;
      return Math.abs(a.match.start - positionHint) - Math.abs(b.match.start - positionHint);
    })[0];

  // Precision gate — reuse the quote path's context-confidence check. isUnique
  // is false here (as on the position path): "one candidate survived the error
  // budget" is a function of maxErrors, not of the text's actual ambiguity, so
  // it must not relax the short-quote both-sides rule.
  const occ: QuoteOccurrence = {
    index: best.match.start,
    isUnique: false,
    prefixScore: best.prefixScore,
    suffixScore: best.suffixScore,
  };
  if (!isConfidentMatch(occ, exact.length, prefix, suffix)) return null;

  // Anchor to the matcher's real end offset, so the highlight spans the actual
  // matched region rather than a fixed-length window.
  return rangeAtTextOffset(index, best.match.start, best.match.end);
}
