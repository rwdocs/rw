import { describe, it, expect, vi } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/RangeRectHarness.svelte";
import { makeRange } from "./__fixtures__/resize-observer-mock";

describe("useRangeRect", () => {
  it("populates the rect fields from getBoundingClientRect on mount", () => {
    const range = makeRange({ top: 10, left: 20, width: 100, height: 50 });

    const { getByTestId } = render(Harness, { range });
    const out = getByTestId("range-rect");

    expect(out.dataset.top).toBe("10");
    expect(out.dataset.left).toBe("20");
    expect(out.dataset.width).toBe("100");
    expect(out.dataset.height).toBe("50");
    expect(out.dataset.measured).toBe("true");
  });

  it("leaves measured=false when the range is null", () => {
    const { getByTestId } = render(Harness, { range: null });
    const out = getByTestId("range-rect");

    expect(out.dataset.measured).toBe("false");
  });

  it("re-measures when window scrolls", () => {
    let current: Partial<DOMRect> = { top: 100, left: 50, width: 100, height: 50 };
    const range = makeRange(() => current);

    const { getByTestId } = render(Harness, { range });
    const out = getByTestId("range-rect");
    expect(out.dataset.top).toBe("100");

    current = { top: 20, left: 50, width: 100, height: 50 };
    window.dispatchEvent(new Event("scroll"));
    flushSync();

    expect(out.dataset.top).toBe("20");
  });

  it("re-measures on ancestor scroll-container scrolls (capture phase)", () => {
    let current: Partial<DOMRect> = { top: 100, left: 50, width: 100, height: 50 };
    const range = makeRange(() => current);
    const scroller = document.createElement("div");
    document.body.appendChild(scroller);

    try {
      const { getByTestId } = render(Harness, { range });
      const out = getByTestId("range-rect");
      expect(out.dataset.top).toBe("100");

      current = { top: 40, left: 50, width: 100, height: 50 };
      scroller.dispatchEvent(new Event("scroll", { bubbles: false }));
      flushSync();

      expect(out.dataset.top).toBe("40");
    } finally {
      scroller.remove();
    }
  });

  it("re-measures when window resizes", () => {
    let current: Partial<DOMRect> = { top: 10, left: 10, width: 100, height: 50 };
    const range = makeRange(() => current);

    const { getByTestId } = render(Harness, { range });
    const out = getByTestId("range-rect");
    expect(out.dataset.left).toBe("10");

    current = { top: 10, left: 200, width: 100, height: 50 };
    window.dispatchEvent(new Event("resize"));
    flushSync();

    expect(out.dataset.left).toBe("200");
  });

  it("removes window listeners when the range becomes null", async () => {
    const range = makeRange({ top: 10, left: 10, width: 100, height: 50 });
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { rerender, getByTestId } = render(Harness, { range });
    const out = getByTestId("range-rect");
    expect(out.dataset.measured).toBe("true");

    await rerender({ range: null });

    const removed = removeSpy.mock.calls
      .filter(([type]) => type === "scroll" || type === "resize")
      .map(([type]) => type);
    expect(removed).toContain("scroll");
    expect(removed).toContain("resize");
    expect(out.dataset.measured).toBe("false");

    removeSpy.mockRestore();
  });

  it("removes window listeners on unmount", () => {
    const range = makeRange({ top: 10, left: 10, width: 100, height: 50 });
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { unmount } = render(Harness, { range });
    unmount();

    const removed = removeSpy.mock.calls
      .filter(([type]) => type === "scroll" || type === "resize")
      .map(([type]) => type);
    expect(removed).toContain("scroll");
    expect(removed).toContain("resize");

    removeSpy.mockRestore();
  });

  it("re-subscribes when the range prop changes", async () => {
    const rangeA = makeRange({ top: 10, left: 0, width: 100, height: 20 });
    const rangeB = makeRange({ top: 500, left: 0, width: 30, height: 40 });

    const { getByTestId, rerender } = render(Harness, { range: rangeA });
    const out = getByTestId("range-rect");
    expect(out.dataset.top).toBe("10");
    expect(out.dataset.width).toBe("100");

    await rerender({ range: rangeB });

    expect(out.dataset.top).toBe("500");
    expect(out.dataset.width).toBe("30");
  });
});
