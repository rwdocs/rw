import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/ElementSizeHarness.svelte";
import { MockResizeObserver, makeAnchor } from "./__fixtures__/resize-observer-mock";

describe("useElementSize", () => {
  beforeEach(() => {
    MockResizeObserver.instances = [];
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("populates width/height from getBoundingClientRect on mount", () => {
    const el = makeAnchor({ width: 100, height: 50 });

    const { getByTestId } = render(Harness, { el });
    const out = getByTestId("element-size");

    expect(out.dataset.width).toBe("100");
    expect(out.dataset.height).toBe("50");
    expect(out.dataset.version).toBe("1");
  });

  it("leaves width/height at zero when the element is null", () => {
    const { getByTestId } = render(Harness, { el: null });
    const out = getByTestId("element-size");

    expect(out.dataset.width).toBe("0");
    expect(out.dataset.height).toBe("0");
    expect(out.dataset.version).toBe("0");
    expect(MockResizeObserver.instances).toHaveLength(0);
  });

  it("updates width/height when ResizeObserver fires", () => {
    let currentRect = { width: 100, height: 50 };
    const el = document.createElement("div");
    el.getBoundingClientRect = () =>
      ({
        ...currentRect,
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect;

    const { getByTestId } = render(Harness, { el });
    const out = getByTestId("element-size");
    expect(out.dataset.width).toBe("100");
    expect(out.dataset.version).toBe("1");

    currentRect = { width: 400, height: 60 };
    MockResizeObserver.instances[0].trigger();
    flushSync();

    expect(out.dataset.width).toBe("400");
    expect(out.dataset.height).toBe("60");
    expect(out.dataset.version).toBe("2");
  });

  it("does not subscribe to window scroll or resize", () => {
    const el = makeAnchor({ width: 100, height: 50 });
    const addSpy = vi.spyOn(window, "addEventListener");

    render(Harness, { el });
    const subscribed = addSpy.mock.calls
      .filter(([type]) => type === "scroll" || type === "resize")
      .map(([type]) => type);
    expect(subscribed).toHaveLength(0);

    addSpy.mockRestore();
  });

  it("disconnects the observer on unmount", () => {
    const el = makeAnchor({ width: 10, height: 10 });

    const { unmount } = render(Harness, { el });
    const observer = MockResizeObserver.instances[0];
    expect(observer.disconnected).toBe(false);

    unmount();
    expect(observer.disconnected).toBe(true);
  });

  it("re-subscribes when the element prop changes", async () => {
    const elA = makeAnchor({ width: 100, height: 20 });
    const elB = makeAnchor({ width: 30, height: 40 });

    const { getByTestId, rerender } = render(Harness, { el: elA });
    const out = getByTestId("element-size");
    expect(out.dataset.width).toBe("100");
    expect(MockResizeObserver.instances).toHaveLength(1);
    const firstObserver = MockResizeObserver.instances[0];

    await rerender({ el: elB });

    expect(firstObserver.disconnected).toBe(true);
    expect(MockResizeObserver.instances).toHaveLength(2);
    expect(out.dataset.width).toBe("30");
    expect(out.dataset.height).toBe("40");
  });
});
