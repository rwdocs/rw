import type { ElementSize } from "./useElementSize.svelte";
import { rangeTouchesDiagram } from "$lib/comments/diagram";

/**
 * Tracks the text selection that drives the Add-comment popover: owns the
 * captured `Range` and the article-relative anchor point the popover renders at.
 *
 * The hook watches the document for selections itself. It captures on a
 * document-level `mouseup` — so a selection released anywhere, including outside
 * the article when the user drags a line's first word right-to-left and lets go
 * in the left gutter, is still caught (a plain article `mouseup` handler never
 * fires for those) — and dismisses on `selectionchange`-collapse. A captured
 * range is kept only when it lies inside the article. The caller owns
 * highlight-click detection (a collapsed click), which needs the in-article
 * `MouseEvent`; the `mouseup` capture is gated on `isEnabled` so the listener is
 * attached only while comments are on.
 *
 * `pos` is the horizontal centre / top edge of the selection, measured relative
 * to the article element — the popover renders `position: absolute` inside the
 * article's wrapper, so these coordinates are scroll-invariant and only need
 * recomputing when the selection changes or the article reflows
 * (`articleSize.version`), never on scroll.
 */
export interface SelectionPopover {
  /** Article-relative anchor point, or `null` when there is no selection. */
  readonly pos: { x: number; y: number } | null;
  /** Drop the current selection. */
  clear(): void;
}

export function useSelectionPopover(
  getArticle: () => HTMLElement | null,
  articleSize: ElementSize,
  isEnabled: () => boolean,
): SelectionPopover {
  let range: Range | null = $state.raw(null);
  let pos: { x: number; y: number } | null = $state.raw(null);

  // Clone-and-keep a fresh selection Range iff it lies inside the article.
  // `Selection.getRangeAt` returns a live Range that mutates on re-selection, so
  // it must be cloned. A range outside the article (sidebar, comment panel, or
  // one spanning the article boundary so its commonAncestorContainer is a shared
  // ancestor like <body>) drops any current selection — that includes the case
  // where the user extends an in-article selection out past the boundary, which
  // never collapses, so the selectionchange effect would otherwise leave the
  // popover stranded at the old anchor.
  function captureRange(r: Range) {
    const article = getArticle();
    if (!article || !article.contains(r.commonAncestorContainer)) {
      range = null;
      return;
    }
    // Diagrams are off-limits to comments: a selection inside, ending in, or
    // spanning a diagram figure never opens the popover.
    if (rangeTouchesDiagram(r, article)) {
      range = null;
      return;
    }
    range = r.cloneRange();
  }

  // Article-relative anchor point. Scroll-invariant by construction, so this
  // recomputes only when the selection changes or the article reflows — never on
  // scroll, which is what keeps the popover lag-free.
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

  // Capture on `mouseup` anywhere in the document, so a selection released
  // outside the article — a right-to-left drag of a line's first word that ends
  // in the left gutter, or any release past the article's edges — is still
  // caught. Attached only while comments are enabled; reading isEnabled() makes
  // the effect re-run (attach/detach) when the flag flips as config arrives.
  $effect(() => {
    if (!isEnabled()) {
      // Comments off: there is no popover surface, so drop any captured range
      // too — otherwise it could re-surface stale if comments are re-enabled
      // later in the same session (an embedding host toggling the feature).
      range = null;
      return;
    }
    const handler = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
      captureRange(sel.getRangeAt(0));
    };
    document.addEventListener("mouseup", handler);
    return () => document.removeEventListener("mouseup", handler);
  });

  // Dismiss when the selection collapses (e.g. the user clicks on the selected
  // text). Blink runs the click-on-selection collapse as a default action of
  // `click`, so reading window.getSelection() inside `mouseup` still returns the
  // active range — the document mouseup listener sees the live selection, and
  // this handler then clears it once the click collapses it.
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
    clear() {
      range = null;
    },
  };
}
