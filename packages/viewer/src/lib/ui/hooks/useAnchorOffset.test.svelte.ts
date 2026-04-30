import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/AnchorOffsetHarness.svelte";
import { MockResizeObserver, makeAnchor, makeRange } from "./__fixtures__/resize-observer-mock";

describe("useAnchorOffset", () => {
  beforeEach(() => {
    MockResizeObserver.instances = [];
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("populates the rect fields from getBoundingClientRect on mount", () => {
    const anchor = makeAnchor({ top: 10, left: 20, width: 100, height: 50 });

    const { getByTestId } = render(Harness, { el: anchor });
    const out = getByTestId("anchor-offset");

    expect(out.dataset.top).toBe("10");
    expect(out.dataset.left).toBe("20");
    expect(out.dataset.width).toBe("100");
    expect(out.dataset.height).toBe("50");
  });

  it("leaves the rect fields at zero when the anchor is null", () => {
    const { getByTestId } = render(Harness, { el: null });
    const out = getByTestId("anchor-offset");

    expect(out.dataset.top).toBe("0");
    expect(out.dataset.left).toBe("0");
    expect(out.dataset.width).toBe("0");
    expect(out.dataset.height).toBe("0");
    expect(out.dataset.measured).toBe("false");
    expect(MockResizeObserver.instances).toHaveLength(0);
  });

  it("flips measured to true after the initial synchronous measurement", () => {
    const anchor = makeAnchor({ top: 10, left: 20, width: 100, height: 50 });

    const { getByTestId } = render(Harness, { el: anchor });
    const out = getByTestId("anchor-offset");

    expect(out.dataset.measured).toBe("true");
  });

  it("flips measured back to false when the anchor becomes null after measure", async () => {
    const anchor = makeAnchor({ top: 10, left: 20, width: 100, height: 50 });

    const { getByTestId, rerender } = render(Harness, { el: anchor });
    const out = getByTestId("anchor-offset");
    expect(out.dataset.measured).toBe("true");

    await rerender({ el: null });

    expect(out.dataset.measured).toBe("false");
  });

  it("keeps measured=true when the anchor switches to a new one", async () => {
    const anchorA = makeAnchor({ top: 10, width: 100, height: 20 });
    const anchorB = makeAnchor({ top: 500, width: 30, height: 40 });

    const { getByTestId, rerender } = render(Harness, { el: anchorA });
    const out = getByTestId("anchor-offset");
    expect(out.dataset.measured).toBe("true");

    await rerender({ el: anchorB });

    expect(out.dataset.measured).toBe("true");
  });

  it("updates the rect fields when ResizeObserver fires", () => {
    let currentRect = { top: 0, left: 0, width: 100, height: 50 };
    const anchor = document.createElement("div");
    anchor.getBoundingClientRect = () =>
      ({ ...currentRect, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;

    const { getByTestId } = render(Harness, { el: anchor });
    const out = getByTestId("anchor-offset");
    expect(out.dataset.top).toBe("0");
    expect(out.dataset.width).toBe("100");

    currentRect = { top: 200, left: 300, width: 400, height: 60 };
    MockResizeObserver.instances[0].trigger();
    flushSync();

    expect(out.dataset.top).toBe("200");
    expect(out.dataset.left).toBe("300");
    expect(out.dataset.width).toBe("400");
    expect(out.dataset.height).toBe("60");
  });

  it("disconnects the observer on unmount", () => {
    const anchor = makeAnchor({ width: 10, height: 10 });

    const { unmount } = render(Harness, { el: anchor });
    const observer = MockResizeObserver.instances[0];
    expect(observer.disconnected).toBe(false);

    unmount();
    expect(observer.disconnected).toBe(true);
  });

  it("updates the rect fields when window scrolls", () => {
    let currentRect = { top: 100, left: 50, width: 100, height: 50 };
    const anchor = document.createElement("div");
    anchor.getBoundingClientRect = () =>
      ({ ...currentRect, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;

    const { getByTestId } = render(Harness, { el: anchor });
    const out = getByTestId("anchor-offset");
    expect(out.dataset.top).toBe("100");

    currentRect = { top: 20, left: 50, width: 100, height: 50 };
    window.dispatchEvent(new Event("scroll"));
    flushSync();

    expect(out.dataset.top).toBe("20");
  });

  it("updates the rect fields when an ancestor scroll-container scrolls (capture phase)", () => {
    let currentRect = { top: 100, left: 50, width: 100, height: 50 };
    const scroller = document.createElement("div");
    const anchor = document.createElement("div");
    scroller.appendChild(anchor);
    document.body.appendChild(scroller);
    anchor.getBoundingClientRect = () =>
      ({ ...currentRect, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;

    try {
      const { getByTestId } = render(Harness, { el: anchor });
      const out = getByTestId("anchor-offset");
      expect(out.dataset.top).toBe("100");

      currentRect = { top: 40, left: 50, width: 100, height: 50 };
      scroller.dispatchEvent(new Event("scroll", { bubbles: false }));
      flushSync();

      expect(out.dataset.top).toBe("40");
    } finally {
      scroller.remove();
    }
  });

  it("updates the rect fields when the window resizes", () => {
    let currentRect = { top: 10, left: 10, width: 100, height: 50 };
    const anchor = document.createElement("div");
    anchor.getBoundingClientRect = () =>
      ({ ...currentRect, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;

    const { getByTestId } = render(Harness, { el: anchor });
    const out = getByTestId("anchor-offset");
    expect(out.dataset.left).toBe("10");

    currentRect = { top: 10, left: 200, width: 100, height: 50 };
    window.dispatchEvent(new Event("resize"));
    flushSync();

    expect(out.dataset.left).toBe("200");
  });

  it("removes window listeners on unmount", () => {
    const anchor = makeAnchor({ top: 10, left: 10, width: 100, height: 50 });
    const addSpy = vi.spyOn(window, "addEventListener");
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { unmount } = render(Harness, { el: anchor });
    const added = addSpy.mock.calls
      .filter(([type]) => type === "scroll" || type === "resize")
      .map(([type]) => type);
    expect(added).toContain("scroll");
    expect(added).toContain("resize");

    unmount();
    const removed = removeSpy.mock.calls
      .filter(([type]) => type === "scroll" || type === "resize")
      .map(([type]) => type);
    expect(removed).toContain("scroll");
    expect(removed).toContain("resize");

    addSpy.mockRestore();
    removeSpy.mockRestore();
  });

  it("re-subscribes when the anchor prop changes", async () => {
    const anchorA = makeAnchor({ top: 10, width: 100, height: 20 });
    const anchorB = makeAnchor({ top: 500, width: 30, height: 40 });

    const { getByTestId, rerender } = render(Harness, { el: anchorA });
    const out = getByTestId("anchor-offset");
    expect(out.dataset.top).toBe("10");
    expect(out.dataset.width).toBe("100");
    expect(MockResizeObserver.instances).toHaveLength(1);
    const firstObserver = MockResizeObserver.instances[0];

    await rerender({ el: anchorB });

    expect(firstObserver.disconnected).toBe(true);
    expect(MockResizeObserver.instances).toHaveLength(2);
    expect(out.dataset.top).toBe("500");
    expect(out.dataset.width).toBe("30");
  });

  // Range targets share the same window scroll/resize plumbing as Element
  // anchors — most behavior is covered above. These tests pin the
  // Range-specific surface: no ResizeObserver subscription, and re-measure
  // on window scroll still works.
  describe("with a Range anchor", () => {
    it("populates the rect fields without subscribing to ResizeObserver", () => {
      const anchor = makeRange({ top: 10, left: 20, width: 100, height: 50 });

      const { getByTestId } = render(Harness, { el: anchor });
      const out = getByTestId("anchor-offset");

      expect(out.dataset.top).toBe("10");
      expect(out.dataset.measured).toBe("true");
      expect(MockResizeObserver.instances).toHaveLength(0);
    });

    it("re-measures when window scrolls", () => {
      let current: Partial<DOMRect> = { top: 100, left: 50, width: 100, height: 50 };
      const anchor = makeRange(() => current);

      const { getByTestId } = render(Harness, { el: anchor });
      const out = getByTestId("anchor-offset");
      expect(out.dataset.top).toBe("100");

      current = { top: 20, left: 50, width: 100, height: 50 };
      window.dispatchEvent(new Event("scroll"));
      flushSync();

      expect(out.dataset.top).toBe("20");
    });
  });
});
