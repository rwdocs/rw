/**
 * The element that scrolls `el`: a host app (Backstage) may own the scroller,
 * leaving the window still.
 */
export function scrollerOf(el: Element | null | undefined): Element | Window {
  for (let node = el?.parentElement; node; node = node.parentElement) {
    // Unless html clips on either axis, body's overflow belongs to the
    // viewport, and body itself never scrolls.
    if (node === document.body) {
      const html = getComputedStyle(document.documentElement);
      if (html.overflowX === "visible" && html.overflowY === "visible") break;
    }
    const { overflowY } = getComputedStyle(node);
    const scrollable = overflowY === "auto" || overflowY === "scroll";
    if (scrollable && node.scrollHeight > node.clientHeight) return node;
  }
  return window;
}
