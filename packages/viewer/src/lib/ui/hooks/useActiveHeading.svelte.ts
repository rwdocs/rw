import { untrack } from "svelte";

/**
 * Scroll-spy: track which heading the reader is currently on, based on
 * IntersectionObserver visibility.
 *
 * Given a reactive list of heading ids (document order), returns a handle
 * whose `activeId` stays in sync with the topmost visible heading as the user
 * scrolls. Consumers that perform programmatic scrolls — click-to-anchor,
 * hash navigation, popstate — own the imperative side: they call
 * `setActiveId` to update the handle immediately, then
 * `suppressUntilScrollEnd` to hold the observer at bay until the browser
 * finishes scrolling, so the observer's own callback does not overwrite the
 * newly selected id mid-flight.
 *
 * The hook re-subscribes whenever `headingIds()` returns a different set;
 * when the list changes and the current `activeId` is no longer in it, it
 * resets to the first id so the UI never points at a heading that is no
 * longer rendered.
 */
export interface ActiveHeading {
  readonly activeId: string | null;
  setActiveId(id: string | null): void;
  suppressUntilScrollEnd(): void;
}

export function useActiveHeading(headingIds: () => string[]): ActiveHeading {
  let activeId = $state<string | null>(null);
  let suppressed = false;

  function suppressUntilScrollEnd(): void {
    suppressed = true;
    const scrollBefore = window.scrollY;
    requestAnimationFrame(() => {
      if (window.scrollY === scrollBefore) {
        suppressed = false;
        return;
      }
      // `scrollend` is the reliable signal but it is missing in some browsers
      // and can be skipped when the scroll is interrupted; the 500ms fallback
      // guarantees the suppression always releases.
      const fallback = setTimeout(() => {
        suppressed = false;
      }, 500);
      window.addEventListener(
        "scrollend",
        () => {
          clearTimeout(fallback);
          suppressed = false;
        },
        { once: true },
      );
    });
  }

  $effect(() => {
    const ids = headingIds();
    if (ids.length === 0) return;

    const visible = new Set<string>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            visible.add(entry.target.id);
          } else {
            visible.delete(entry.target.id);
          }
        }
        if (suppressed) return;
        for (const id of ids) {
          if (visible.has(id)) {
            activeId = id;
            return;
          }
        }
        // Nothing visible: keep the last active id. Heading-dense pages often
        // have brief gaps where no heading sits in the rootMargin band; the
        // scroll-spy should feel sticky, not flicker.
      },
      {
        // Match the legacy TocSidebar band: a heading becomes "active" once
        // its top enters the 10-20% strip of the viewport.
        rootMargin: "-10% 0px -80% 0px",
        threshold: 0,
      },
    );

    for (const id of ids) {
      const el = document.getElementById(id);
      if (el) observer.observe(el);
    }

    // Read activeId via untrack so the effect does not depend on its own
    // writes; otherwise every observer-driven update would re-run the effect,
    // tearing down and rebuilding the observer.
    untrack(() => {
      if (activeId === null || !ids.includes(activeId)) {
        activeId = ids[0];
      }
    });

    return () => observer.disconnect();
  });

  return {
    get activeId() {
      return activeId;
    },
    setActiveId(id: string | null) {
      activeId = id;
    },
    suppressUntilScrollEnd,
  };
}
