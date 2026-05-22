import type { ElementSize } from "./useElementSize.svelte";

/**
 * Tracks the text selection that drives the Add-comment popover: owns the
 * captured `Range`, the article-relative anchor point the popover renders at,
 * and dismiss-on-collapse.
 *
 * `pos` is the horizontal centre / top edge of the selection, measured
 * relative to the article element — the popover renders `position: absolute`
 * inside the article's wrapper, so these coordinates are scroll-invariant and
 * only need recomputing when the article reflows (`articleSize.version`).
 *
 * The caller still owns the article element and its `mouseup` handler (that
 * handler is shared with comment-highlight click detection): on a fresh
 * selection it calls `capture(range)`, and `clear()` to dismiss.
 */
export interface SelectionPopover {
  /** Article-relative anchor point, or `null` when there is no selection. */
  readonly pos: { x: number; y: number } | null;
  /**
   * Capture a selection `Range`. The range is cloned — `Selection.getRangeAt`
   * returns a live Range that mutates on re-selection — and ignored unless it
   * lies inside the article.
   */
  capture(range: Range): void;
  /** Drop the current selection. */
  clear(): void;
}

export function useSelectionPopover(
  getArticle: () => HTMLElement | null,
  articleSize: ElementSize,
): SelectionPopover {
  let range: Range | null = $state.raw(null);
  let pos: { x: number; y: number } | null = $state.raw(null);

  // Article-relative anchor point. Scroll-invariant by construction, so this
  // recomputes only when the selection changes or the article reflows — never
  // on scroll, which is what keeps the popover lag-free.
  $effect(() => {
    const r = range;
    const article = getArticle();
    if (!r || !article) {
      pos = null;
      return;
    }
    void articleSize.version;
    const rect = r.getBoundingClientRect();
    const a = article.getBoundingClientRect();
    pos = { x: rect.left + rect.width / 2 - a.left, y: rect.top - a.top };
  });

  // Dismiss when the selection collapses (e.g. the user clicks on the selected
  // text). Blink runs the click-on-selection collapse as a default action of
  // `click`, so reading window.getSelection() inside `mouseup` still returns
  // the active range — the caller would re-capture at the same coords and only
  // the highlight would disappear.
  $effect(() => {
    if (!range) return;
    const handler = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) range = null;
    };
    document.addEventListener("selectionchange", handler);
    return () => document.removeEventListener("selectionchange", handler);
  });

  return {
    get pos() {
      return pos;
    },
    capture(r: Range) {
      const article = getArticle();
      if (!article || !article.contains(r.commonAncestorContainer)) {
        range = null;
        return;
      }
      range = r.cloneRange();
    },
    clear() {
      range = null;
    },
  };
}
