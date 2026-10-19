import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/RelativeOffsetHarness.svelte";
import { MockResizeObserver } from "./__fixtures__/resize-observer-mock";

/** An element whose bounding rect the test can move. */
function box(rect: { top: number; height: number }) {
  const el = document.createElement("div");
  el.getBoundingClientRect = () =>
    ({ ...rect, left: 0, width: 0, right: 0, bottom: rect.top + rect.height }) as DOMRect;
  return { el, rect };
}

const halfHeight = (_: HTMLElement, r: DOMRect) => r.height / 2;

/** Fire every ResizeObserver, as a layout change would. */
function resize() {
  for (const observer of MockResizeObserver.instances) observer.trigger();
  flushSync();
}

describe("useRelativeOffset", () => {
  beforeEach(() => {
    MockResizeObserver.instances = [];
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reports inner top minus outer top plus the inset on mount", () => {
    const outer = box({ top: 100, height: 300 });
    const inner = box({ top: 140, height: 24 });
    const onChange = vi.fn();

    render(Harness, { outer: outer.el, inner: inner.el, inset: halfHeight, onChange });

    expect(onChange).toHaveBeenCalledExactlyOnceWith(52);
  });

  it("passes the inner element and its rect to the inset", () => {
    const outer = box({ top: 0, height: 100 });
    const inner = box({ top: 10, height: 20 });
    const inset = vi.fn(() => 3);
    const onChange = vi.fn();

    render(Harness, { outer: outer.el, inner: inner.el, inset, onChange });

    expect(inset).toHaveBeenCalledWith(inner.el, expect.objectContaining({ top: 10, height: 20 }));
    expect(onChange).toHaveBeenCalledExactlyOnceWith(13);
  });

  it("re-reports after a resize moves the offset by more than half a pixel", () => {
    const outer = box({ top: 0, height: 300 });
    const inner = box({ top: 40, height: 20 });
    const onChange = vi.fn();
    render(Harness, { outer: outer.el, inner: inner.el, inset: halfHeight, onChange });

    inner.rect.top = 41;
    resize();

    expect(onChange.mock.calls).toEqual([[50], [51]]);
  });

  it("ignores changes of half a pixel or less", () => {
    const outer = box({ top: 0, height: 300 });
    const inner = box({ top: 40, height: 20 });
    const onChange = vi.fn();
    render(Harness, { outer: outer.el, inner: inner.el, inset: halfHeight, onChange });

    inner.rect.top = 40.5;
    resize();

    expect(onChange).toHaveBeenCalledOnce();
  });

  it("measures nothing without a callback", () => {
    const outer = box({ top: 0, height: 300 });
    const inner = box({ top: 40, height: 20 });
    const inset = vi.fn(() => 0);

    render(Harness, { outer: outer.el, inner: inner.el, inset });
    resize();

    expect(inset).not.toHaveBeenCalled();
  });
});
