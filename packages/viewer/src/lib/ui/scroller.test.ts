import { describe, it, expect, afterEach } from "vitest";
import { scrollerOf } from "./scroller";

/** jsdom has no layout, so give `el` the overflow and extent of a real scroller. */
function overflowing(el: HTMLElement, overflowY: string) {
  el.style.overflowY = overflowY;
  Object.defineProperty(el, "scrollHeight", { configurable: true, value: 2000 });
  Object.defineProperty(el, "clientHeight", { configurable: true, value: 500 });
}

function reset(el: HTMLElement) {
  el.removeAttribute("style");
  delete (el as { scrollHeight?: number }).scrollHeight;
  delete (el as { clientHeight?: number }).clientHeight;
}

describe("scrollerOf", () => {
  afterEach(() => {
    reset(document.documentElement);
    reset(document.body);
    document.body.replaceChildren();
  });

  function mount(): { pane: HTMLElement; leaf: HTMLElement } {
    const pane = document.createElement("div");
    const leaf = document.createElement("p");
    pane.append(leaf);
    document.body.append(pane);
    return { pane, leaf };
  }

  it("is the window when no ancestor scrolls", () => {
    expect(scrollerOf(mount().leaf)).toBe(window);
  });

  it("is a host-owned pane that scrolls", () => {
    const { pane, leaf } = mount();
    overflowing(pane, "auto");
    expect(scrollerOf(leaf)).toBe(pane);
  });

  it("skips a pane that could scroll but fits its content", () => {
    const { pane, leaf } = mount();
    pane.style.overflowY = "auto";
    expect(scrollerOf(leaf)).toBe(window);
  });

  it("is the window when body's overflow propagates to the viewport", () => {
    overflowing(document.body, "auto");
    expect(scrollerOf(mount().leaf)).toBe(window);
  });

  it("is body when html clips only horizontally", () => {
    document.documentElement.style.overflowX = "clip";
    overflowing(document.body, "auto");
    expect(scrollerOf(mount().leaf)).toBe(document.body);
  });

  it("is body when html clips, so body scrolls itself", () => {
    document.documentElement.style.overflowY = "hidden";
    overflowing(document.body, "auto");
    expect(scrollerOf(mount().leaf)).toBe(document.body);
  });
});
