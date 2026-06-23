/**
 * Run `onMove(el)` whenever the element's *position* changes — even when its
 * own box does not resize. Re-subscribes automatically when the getter starts
 * returning a different element, and tears down on cleanup.
 *
 * This fills the gap a `ResizeObserver` cannot: when content *above* an element
 * reflows (web-font swap / FOUT, a late-loading image or diagram, an expanding
 * block), the element slides down without its own width/height changing — so a
 * ResizeObserver on it (or on an ancestor whose box is unchanged) never fires.
 * Positioning that is anchored to such an element (a comment thread pinned to
 * its highlight) would otherwise go stale.
 *
 * Implemented with an `IntersectionObserver` whose `rootMargin` is computed to
 * frame the element exactly, so the observed ratio drops the instant the
 * element moves; the callback then re-measures and re-arms. This is the
 * mechanism behind Floating UI's `autoUpdate({ layoutShift: true })`, ported
 * here (MIT-licensed) so the viewer needs no extra dependency.
 *
 * Like a scroll listener it also fires on scroll (the element moves in the
 * viewport); consumers whose computed value is scroll-invariant simply
 * recompute the same value — cheap and idempotent.
 *
 * Implementation detail of the hooks/ layer — not exported outside it.
 */
export function observeMove(
  getEl: () => HTMLElement | null,
  onMove: (el: HTMLElement) => void,
): void {
  $effect(() => {
    const el = getEl();
    if (!el || typeof IntersectionObserver === "undefined") return;

    let alive = true;
    let io: IntersectionObserver | null = null;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    const root = el.ownerDocument.documentElement;

    function cleanup() {
      if (timeoutId !== undefined) clearTimeout(timeoutId);
      timeoutId = undefined;
      io?.disconnect();
      io = null;
    }

    function refresh(skip: boolean, threshold: number) {
      cleanup();
      if (!alive || !el) return;

      const { left, top, width, height } = el.getBoundingClientRect();
      if (!skip) onMove(el);
      // A zero-area (detached / display:none) element can't be framed; bail and
      // wait for the next re-subscribe to re-arm once it has a box again.
      if (!width || !height) return;

      // Negative insets frame the element exactly, so threshold=1 means "fully
      // in view at this precise position". Any movement drops the ratio.
      const rootMargin =
        `${-Math.floor(top)}px ${-Math.floor(root.clientWidth - (left + width))}px ` +
        `${-Math.floor(root.clientHeight - (top + height))}px ${-Math.floor(left)}px`;

      let isFirstUpdate = true;
      const handle = (entries: IntersectionObserverEntry[]) => {
        // A delivery already queued when the effect tore down must not re-arm a
        // timer on the dead closure (cleanup has already run and won't clear it).
        if (!alive) return;
        // Use the most recent entry; a batched callback is not ordered latest-last.
        const ratio = entries[entries.length - 1].intersectionRatio;
        if (ratio !== threshold) {
          if (!isFirstUpdate) {
            refresh(false, 1);
          } else if (ratio) {
            // Re-arm at the actual ratio so the next genuine move retriggers.
            refresh(false, ratio);
          } else {
            // Ratio 0 (e.g. scrolled fully off-screen): poll back in a beat.
            timeoutId = setTimeout(() => refresh(false, 1e-7), 1000);
          }
        }
        isFirstUpdate = false;
      };

      const options: IntersectionObserverInit = {
        rootMargin,
        threshold: Math.max(0, Math.min(1, threshold)) || 1,
      };
      try {
        // Observe relative to the document so an ancestor's reflow is caught.
        io = new IntersectionObserver(handle, { ...options, root: el.ownerDocument });
      } catch {
        io = new IntersectionObserver(handle, options);
      }
      io.observe(el);
    }

    refresh(true, 1);

    return () => {
      alive = false;
      cleanup();
    };
  });
}
