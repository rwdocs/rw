import {
  buildTextIndex,
  selectorsToRange,
  selectorsToRangeIn,
  type AnchorStrategy,
} from "$lib/anchoring";
import type { Selector, Comment } from "../../types/comments";
import { wrapRange, unwrapComment, escapeId } from "./highlight";
import { rangeIntersectsNode, rangesOverlap } from "./ranges";

export interface DesiredComment {
  id: string;
  selectors: Selector[];
  /** Absent/undefined for a top-level thread, set for a reply. Only top-level
   *  comments that fail to anchor are promoted to orphans, so a falsy parentId
   *  (undefined from the API, or null) means "top-level". */
  parentId?: string | null;
}

/**
 * The set of comments whose passages should be highlighted in the article.
 * A comment is highlighted when it is unresolved OR it is the currently active
 * thread — so a just-resolved comment keeps its highlight (and its document-order
 * slot in `order`) until the reader navigates away. Comments without selectors
 * (page-level comments, replies) are never highlighted.
 */
export function desiredHighlights(items: Comment[], activeId: string | null): DesiredComment[] {
  return items
    .filter((c) => (c.status !== "resolved" || c.id === activeId) && c.selectors.length > 0)
    .map((c) => ({ id: c.id, selectors: c.selectors, parentId: c.parentId }));
}

export interface ReconcileResult {
  ranges: Map<string, Range>;
  strategies: Map<string, AnchorStrategy>;
  orphanIds: Set<string>;
  /** Anchored ids in live DOM order (first wrapper occurrence). */
  order: string[];
  /** Whether a wrap/unwrap touched the passed-in selection range. */
  touchesSelection: boolean;
}

/**
 * Reconcile the article's `<rw-annotation>` highlights to match `desired`,
 * mutating only what changed. The DOM is the wrapped-set ledger: ids present in
 * the DOM but absent from `desired` are unwrapped; desired ids not yet in the
 * DOM are anchored and wrapped. A re-rendered article (navigation / live-reload)
 * has zero wrappers, so this naturally re-adds everything — no separate path.
 *
 * `selection` is the user's current non-collapsed selection (or null). The
 * result's `touchesSelection` is true only if a wrap/unwrap actually overlapped
 * it, so the caller clears the selection only when the mutation would have
 * disturbed it.
 */
export function reconcileHighlights(
  container: HTMLElement,
  desired: DesiredComment[],
  selection: Range | null,
): ReconcileResult {
  const desiredById = new Map(desired.map((c) => [c.id, c]));
  const wrappedIds = new Set<string>();
  for (const el of container.querySelectorAll("rw-annotation")) {
    const id = el.getAttribute("data-comment-id");
    // Guard against a stray wrapper without our attribute (e.g. authored in raw
    // markdown): a null id would otherwise pollute the ledger and force a full
    // re-anchor every pass.
    if (id) wrappedIds.add(id);
  }

  let touchesSelection = false;
  let mutated = false;

  // 1. Unwrap ids that are wrapped but no longer desired.
  for (const id of wrappedIds) {
    if (desiredById.has(id)) continue;
    if (selection) {
      for (const el of container.querySelectorAll(
        `rw-annotation[data-comment-id="${escapeId(id)}"]`,
      )) {
        if (rangeIntersectsNode(selection, el)) {
          touchesSelection = true;
          break;
        }
      }
    }
    unwrapComment(container, id);
    mutated = true;
  }

  // 2. Re-anchor ALL desired comments (not just newly-added ones) against one
  //    freshly built index, built after the unwraps. Re-anchoring everything
  //    rebuilds strategies and orphanIds from scratch each pass, so no stale
  //    entry from a previous pass can linger. The ranges captured here are used
  //    to wrap the added ids; the stored `ranges` map is rebuilt from the final
  //    DOM after wrapping (see below), because wrapping collapses pre-split
  //    Range objects.
  const index = buildTextIndex(container);
  const ranges = new Map<string, Range>();
  const strategies = new Map<string, AnchorStrategy>();
  const orphanIds = new Set<string>();
  const toWrap: string[] = [];

  for (const comment of desired) {
    const result = selectorsToRangeIn(comment.selectors, index);
    if (!result) {
      if (!comment.parentId) orphanIds.add(comment.id);
      continue;
    }
    ranges.set(comment.id, result.range);
    strategies.set(comment.id, result.strategy);
    if (!wrappedIds.has(comment.id)) {
      toWrap.push(comment.id);
    }
  }

  if (toWrap.length > 0) mutated = true;

  // 3. Wrap the desired ids not already in the DOM. Re-resolve each against the
  //    LIVE DOM immediately before wrapping rather than reusing the range from
  //    step 2: wrapRange splits text nodes, so once an earlier overlapping
  //    comment in this same batch is wrapped, a range captured against the
  //    pre-wrap DOM is collapsed/invalidated. Resolving against the
  //    progressively-wrapped DOM keeps overlapping comments nesting correctly
  //    (the common case where >1 comment is wrapped in one pass is the initial
  //    load / a re-rendered article).
  for (const id of toWrap) {
    const comment = desiredById.get(id);
    if (!comment) continue;
    const result = selectorsToRange(comment.selectors, container);
    if (!result) {
      // Lost its anchor between step 2 and now (only possible if an earlier wrap
      // in this batch perturbed shared text past recognition) — orphan it.
      ranges.delete(id);
      strategies.delete(id);
      if (!comment.parentId) orphanIds.add(id);
      continue;
    }
    if (selection && rangesOverlap(selection, result.range)) touchesSelection = true;
    const wrappers = wrapRange(result.range, { commentId: id, strategy: result.strategy });
    if (wrappers.length === 0) {
      // Whitespace-only range: no visible wrapper. Treat top-level as orphan and
      // drop from the anchored maps so it surfaces in the page timeline.
      ranges.delete(id);
      strategies.delete(id);
      if (!comment.parentId) orphanIds.add(id);
    }
  }

  // Wrapping (and unwrap normalize) splits/merges text nodes, which collapses
  // Range objects created before the mutation — including the ranges stored above
  // for comments that share a text node with a freshly wrapped one. Rebuild the
  // stored ranges from the final DOM so commentRanges holds valid, non-collapsed
  // ranges (consumed by getHighlightAnchor). textContent is unchanged by wrapping, so
  // re-anchoring resolves the same spans with live post-mutation nodes. Only needed
  // when the DOM actually changed.
  if (mutated) {
    const finalIndex = buildTextIndex(container);
    for (const comment of desired) {
      if (orphanIds.has(comment.id)) continue;
      const result = selectorsToRangeIn(comment.selectors, finalIndex);
      if (result) ranges.set(comment.id, result.range);
      else ranges.delete(comment.id);
    }
  }

  // 4. Derive order from live DOM (document order, first wrapper per id).
  const order: string[] = [];
  const seen = new Set<string>();
  for (const el of container.querySelectorAll("rw-annotation")) {
    const id = el.getAttribute("data-comment-id");
    if (id && !seen.has(id)) {
      seen.add(id);
      order.push(id);
    }
  }

  return { ranges, strategies, orphanIds, order, touchesSelection };
}
