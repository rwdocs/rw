/**
 * Ancestors of `el` that are allowed to scroll it, nearest first, whether or
 * not they currently do.
 */
export function scrollableAncestors(el: Element | null | undefined): HTMLElement[] {
  const found: HTMLElement[] = [];
  for (let node = el?.parentElement; node; node = node.parentElement) {
    // Unless html clips on either axis, body's overflow belongs to the
    // viewport, and body itself never scrolls.
    if (node === document.body) {
      const html = getComputedStyle(document.documentElement);
      if (html.overflowX === "visible" && html.overflowY === "visible") break;
    }
    const { overflowY } = getComputedStyle(node);
    if (overflowY === "auto" || overflowY === "scroll") found.push(node);
  }
  return found;
}

/** Which of `candidates` is scrolling right now, or null for the page. */
export function activeScroller(candidates: HTMLElement[]): HTMLElement | null {
  return candidates.find((node) => node.scrollHeight > node.clientHeight) ?? null;
}

/**
 * The element that scrolls `el`: a host app (Backstage) may own the scroller,
 * leaving the window still.
 */
export function scrollerOf(el: Element | null | undefined): Element | Window {
  return activeScroller(scrollableAncestors(el)) ?? window;
}
