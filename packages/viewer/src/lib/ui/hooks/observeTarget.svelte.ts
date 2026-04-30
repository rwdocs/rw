/**
 * Run `onMeasure(target)` once on subscription and again whenever the target
 * resizes. Re-subscribes automatically when the getter starts returning a
 * different target, and tears down the observer (and any window listeners)
 * on cleanup.
 *
 * `target` can be an `Element` (observed via `ResizeObserver`) or a `Range`
 * (no resize observer — Ranges aren't observable, so re-measurement relies
 * solely on `trackWindow`).
 *
 * `trackWindow`: also re-measure on window scroll (capture phase, so ancestor
 * scroll containers count) and window resize. Use this for hooks that report
 * viewport-relative coordinates; skip it for size-only hooks, since size is
 * scroll-invariant and listening would only produce redundant work.
 *
 * Implementation detail of `useElementSize` / `useAnchorOffset` — not
 * exported outside the hooks/ layer.
 */
export function observeTarget<T extends Element | Range>(
  getTarget: () => T | null,
  onMeasure: (target: T) => void,
  { trackWindow = false }: { trackWindow?: boolean } = {},
): void {
  $effect(() => {
    const target = getTarget();
    if (!target) return;

    const measure = () => onMeasure(target);

    measure();
    let observer: ResizeObserver | null = null;
    if (target instanceof Element) {
      observer = new ResizeObserver(measure);
      observer.observe(target);
    }
    if (trackWindow) {
      window.addEventListener("scroll", measure, { capture: true, passive: true });
      window.addEventListener("resize", measure);
    }
    return () => {
      observer?.disconnect();
      if (trackWindow) {
        window.removeEventListener("scroll", measure, { capture: true });
        window.removeEventListener("resize", measure);
      }
    };
  });
}
