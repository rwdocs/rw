import { untrack } from "svelte";

/** Where "you are here" sits, as a fraction of the viewport height. */
const LINE_FRACTION = 0.2;

/** Slack for fractional device-pixel rounding when testing for the page bottom. */
const BOTTOM_EPSILON_PX = 2;

/**
 * How far the reader has scrolled and how far they could, from whichever
 * element actually scrolls: a container the host app owns, else the page.
 */
function scrollExtent(scroller: HTMLElement | null): { at: number; max: number } {
  if (scroller) {
    return { at: scroller.scrollTop, max: scroller.scrollHeight - scroller.clientHeight };
  }
  return { at: window.scrollY, max: document.documentElement.scrollHeight - window.innerHeight };
}

/**
 * Ancestors that are allowed to scroll the headings, nearest first — whether or
 * not they currently do. Resolved once per heading list, because reading
 * computed styles on the scroll path would force a style recalc every frame.
 */
function scrollableAncestors(ids: string[]): HTMLElement[] {
  const first = ids.map((id) => document.getElementById(id)).find((el) => el !== null);
  const candidates: HTMLElement[] = [];
  for (let node = first?.parentElement; node; node = node.parentElement) {
    const { overflowY } = getComputedStyle(node);
    if (overflowY === "auto" || overflowY === "scroll") candidates.push(node);
  }
  return candidates;
}

/**
 * Which candidate is scrolling the headings right now, or null for the page.
 * Re-asked every time: a container that fitted when the page loaded starts
 * scrolling once a late diagram or web font grows the content inside it.
 */
function activeScroller(candidates: HTMLElement[]): HTMLElement | null {
  return candidates.find((node) => node.scrollHeight > node.clientHeight) ?? null;
}

/**
 * The heading the reader has most recently scrolled past, or null when none of
 * the ids is rendered.
 *
 * The first heading if they have passed none, and the last heading once the
 * document cannot scroll any further — without that, a heading close enough to
 * the end never reaches the line and its outline entry is unreachable.
 */
function resolveActiveHeading(ids: string[], candidates: HTMLElement[]): string | null {
  const scroller = activeScroller(candidates);
  // Measured down from the scrollport, not the window: a host header can push
  // the reading area well below the window's own line, and heading rects are
  // viewport coordinates either way.
  const line = scroller
    ? scroller.getBoundingClientRect().top + scroller.clientHeight * LINE_FRACTION
    : window.innerHeight * LINE_FRACTION;
  let first: HTMLElement | null = null;
  let last: HTMLElement | null = null;
  let passed: HTMLElement | null = null;

  for (const id of ids) {
    const el = document.getElementById(id);
    if (!el) continue;
    const rect = el.getBoundingClientRect();
    // A heading in a collapsed tab panel reports an all-zero rect, which would
    // otherwise read as sitting above the line.
    if (rect.width === 0 && rect.height === 0) continue;
    first ??= el;
    last = el;
    if (rect.top <= line) passed = el;
  }

  if (!first || !last) return null;
  // Ordered ahead of the bottom rule: a page can scroll less than the distance
  // to its first heading, leaving a reader who has bottomed out still above the
  // outline entirely.
  if (!passed) return first.id;

  const { at, max } = scrollExtent(scroller);
  if (max > BOTTOM_EPSILON_PX && at >= max - BOTTOM_EPSILON_PX) {
    return last.id;
  }
  return passed.id;
}

/** The distinct elements holding the headings; order is irrelevant to observing them. */
function headingContainers(ids: string[]): HTMLElement[] {
  const containers = new Set<HTMLElement>();
  for (const id of ids) {
    const parent = document.getElementById(id)?.parentElement;
    if (parent) containers.add(parent);
  }
  return [...containers];
}

/**
 * Track which heading the reader is currently on, for the page outline.
 *
 * Given a reactive list of heading ids (document order), returns a handle whose
 * `activeId` is recomputed from geometry on every scroll frame, on resize, when
 * the page reflows, and whenever the list changes. A new id list resets it, so
 * an entry chosen on one document cannot survive into the next.
 *
 * `choose` is the imperative half, for click-to-anchor and history
 * navigation. It marks the entry current, runs the scroll the caller wants,
 * and outranks geometry until the reader scrolls away from it — the ordering
 * between marking and scrolling is the hook's to get right, since only it
 * knows it must outlast a scroll it did not see begin.
 */
export interface ActiveHeading {
  readonly activeId: string | null;
  /**
   * Mark `id` current and hold geometry off until the page settles on it.
   *
   * `scrollTo` is for a caller that scrolls itself; it runs between arming and
   * measuring. `awaitScroll` is for one that does not — a deep link another
   * component scrolls to a frame later — where there is nothing to run but the
   * scroll still must not read as the reader scrolling away.
   */
  choose(id: string, scroll?: { scrollTo?: () => void; awaitScroll?: boolean }): void;
}

export function useActiveHeading(headingIds: () => string[]): ActiveHeading {
  let activeId = $state<string | null>(null);
  let suppressed = false;
  /** The entry the reader chose, and where it sat once the page came to rest. */
  let pinnedId: string | null = null;
  let pinnedTop = 0;
  /**
   * Where a heading sits, or null when it has no position to be held against:
   * absent, or in a collapsed tab panel, whose all-zero rect a pin taken at 0
   * would go on matching for the rest of the document.
   */
  function topOf(id: string): number | null {
    const rect = document.getElementById(id)?.getBoundingClientRect();
    if (!rect || (rect.width === 0 && rect.height === 0)) return null;
    return rect.top;
  }

  /**
   * Everything a choice schedules and has not yet run: the frame that measures
   * its scroll, and the `scrollend` listener with its fallback timer. A new
   * choice — or teardown — drops them, so a superseded choice can never release
   * the one that replaced it.
   */
  let pendingFrame = 0;
  let detachSettle: (() => void) | null = null;

  function abandonChoice(): void {
    if (pendingFrame !== 0) {
      cancelAnimationFrame(pendingFrame);
      pendingFrame = 0;
    }
    detachSettle?.();
  }

  function release(): void {
    suppressed = false;
    if (pinnedId === null) return;
    // Re-measure: the scroll just moved the chosen entry to its resting place.
    const top = topOf(pinnedId);
    if (top === null) pinnedId = null;
    else pinnedTop = top;
  }

  function choose(id: string, scroll?: { scrollTo?: () => void; awaitScroll?: boolean }): void {
    abandonChoice();
    // Every return below leaves the resolver free unless this call arms it
    // itself; inheriting the abandoned choice's suppression would silence it
    // with nothing left to release it.
    suppressed = false;
    activeId = id;

    const top = topOf(id);
    if (top === null) {
      // Nothing rendered to hold the entry against. Marking it current is all
      // there is to do; geometry answers again on the next frame.
      pinnedId = null;
      return;
    }
    pinnedId = id;
    pinnedTop = top;

    const { scrollTo, awaitScroll } = scroll ?? {};
    if (!scrollTo && !awaitScroll) return;
    suppressed = true;

    if (!scrollTo) {
      // Nothing to run: the scroll is on its way from somewhere else.
      awaitSettle();
      return;
    }

    // Armed before the scroll rather than after it: the scroll steps run ahead
    // of animation-frame callbacks, so an instant `scrollIntoView` has already
    // dispatched its `scrollend` by the time the frame below arrives.
    awaitSettle();
    scrollTo();
    pendingFrame = requestAnimationFrame(() => {
      pendingFrame = 0;
      // Measured on the chosen heading rather than `window.scrollY`, which
      // never moves when a host app owns the scroller.
      if (topOf(id) !== top) return;
      detachSettle?.();
      release();
    });
  }

  function awaitSettle(): void {
    // `scrollend` is the reliable signal, but Safari only shipped it in 26.2
    // and it is skipped when a scroll is interrupted; the 500ms fallback
    // guarantees the suppression always releases. Whichever wins detaches the
    // other, or a browser that never fires `scrollend` accumulates a listener
    // per navigation.
    //
    // Capture phase, for the same reason `scroll` is: targeted at an ancestor
    // scroll container, `scrollend` never reaches a bubbling window listener.
    function settle(): void {
      done();
      release();
    }
    function done(): void {
      clearTimeout(fallback);
      window.removeEventListener("scrollend", settle, { capture: true });
      detachSettle = null;
    }
    const fallback = setTimeout(settle, 500);
    detachSettle = done;
    window.addEventListener("scrollend", settle, { once: true, capture: true });
  }

  $effect(() => {
    const ids = headingIds();
    if (ids.length === 0) return;

    // A suppression or pin held for the previous document must not gate this
    // one's seed, or the id it was protecting survives into a page the reader
    // has only just opened. Do NOT gate this on the ids having changed: sibling
    // documents from one template carry identical heading ids, so the ids
    // cannot tell a new document from a re-derived array.
    suppressed = false;
    pinnedId = null;

    let candidates = scrollableAncestors(ids);

    /**
     * A scroll from an element we do not know about, but which holds the
     * headings, means the host changed which ancestor scrolls — a breakpoint,
     * a panel opening. Re-reading computed styles is a style recalc, so it is
     * gated on that: an unrelated scroller (the nav sidebar, a code block) does
     * not contain a heading and costs nothing.
     */
    function rediscoverScroller(target: EventTarget | null): void {
      if (!(target instanceof HTMLElement) || candidates.includes(target)) return;
      const heading = ids.map((id) => document.getElementById(id)).find((el) => el !== null);
      if (!heading || !target.contains(heading)) return;

      const found = scrollableAncestors(ids);
      // Watch them from here on, or a later resize of the panel moves the line
      // with nothing to notice. Re-observing one already watched is a no-op.
      for (const el of found) reflow.observe(el);
      candidates = found;
    }

    function apply(scrolled: boolean): void {
      if (suppressed) return;
      if (pinnedId !== null) {
        // Only a scroll that actually moved the chosen entry gives it back to
        // geometry. Gating on the event keeps a reflow from stealing it; gating
        // on movement ignores the trailing scroll events a programmatic jump
        // leaves behind. Measured as a viewport-relative rect, not
        // `window.scrollY`, which never moves when a host app owns the scroller.
        const top = topOf(pinnedId);
        // A heading that has stopped rendering — its tab was collapsed — keeps
        // no position to be held against, so geometry takes it back.
        if (top !== null && (!scrolled || top === pinnedTop)) return;
        pinnedId = null;
      }
      const next = resolveActiveHeading(ids, candidates);
      // Nothing rendered to measure — the article is mid-swap while its outline
      // is still on screen. Keep the current entry rather than blanking it.
      if (next === null) return;
      if (next !== activeId) activeId = next;
    }

    let frame = 0;
    let sawScroll = false;
    function schedule(event: Event): void {
      if (event.type === "scroll") {
        sawScroll = true;
        rediscoverScroller(event.target);
      }
      if (frame !== 0) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const scrolled = sawScroll;
        sawScroll = false;
        apply(scrolled);
      });
    }

    // Capture phase: scroll events from an ancestor scroll container never reach
    // a bubbling window listener, and a host app embedding the viewer may own
    // the scroller.
    window.addEventListener("scroll", schedule, { capture: true, passive: true });
    window.addEventListener("resize", schedule);
    // Content above a heading can reflow it past the line with no scroll and no
    // window resize: a late diagram or image, a web-font swap, the comments
    // panel widening the column.
    //
    // Do NOT simplify this to `document.body`: `app.css` pins `html, body,
    // #app` to `height: 100%`, so body's box is the viewport and never grows
    // with the content.
    const reflow = new ResizeObserver(() => schedule(new Event("reflow")));
    for (const container of headingContainers(ids)) {
      reflow.observe(container);
    }
    // The scrollports too: the line is measured from one, so a host resizing
    // its panel — a drawer, a split pane, a header that expands — moves the
    // line without touching the window or the article.
    for (const candidate of candidates) {
      reflow.observe(candidate);
    }

    // Seed synchronously so a new document never paints the previous one's
    // highlight. `untrack` keeps the effect from depending on its own write.
    untrack(() => apply(false));

    return () => {
      abandonChoice();
      if (frame !== 0) cancelAnimationFrame(frame);
      reflow.disconnect();
      window.removeEventListener("scroll", schedule, { capture: true });
      window.removeEventListener("resize", schedule);
    };
  });

  return {
    get activeId() {
      return activeId;
    },
    choose,
  };
}
