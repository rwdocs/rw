import { useElementSize } from "./useElementSize.svelte";

/**
 * Report the vertical distance from `outer`'s border-box top to a point inside
 * `inner` — its top plus `inset(inner, innerRect)` — whenever either element
 * resizes. Changes of half a pixel or less are not reported, so subpixel layout
 * jitter can't loop a caller that feeds the offset back into layout.
 *
 * `getOnChange` is a getter so a component can pass an optional callback prop;
 * nothing is measured while it returns undefined.
 */
export function useRelativeOffset(
  getOuter: () => HTMLElement | undefined,
  getInner: () => HTMLElement | undefined,
  inset: (inner: HTMLElement, innerRect: DOMRect) => number,
  getOnChange: () => ((offsetPx: number) => void) | undefined,
): void {
  const outerSize = useElementSize(() => getOuter() ?? null);
  const innerSize = useElementSize(() => getInner() ?? null);

  let lastReported: number | null = null;
  $effect(() => {
    const onChange = getOnChange();
    const outer = getOuter();
    const inner = getInner();
    if (!onChange || !outer || !inner) return;
    void outerSize.version;
    void innerSize.version;
    // Bounding rects rather than offsetTop: `outer` needn't be positioned, so
    // it isn't necessarily `inner`'s offsetParent.
    const innerRect = inner.getBoundingClientRect();
    const offset = innerRect.top - outer.getBoundingClientRect().top + inset(inner, innerRect);
    if (lastReported === null || Math.abs(offset - lastReported) > 0.5) {
      lastReported = offset;
      onChange(offset);
    }
  });
}
