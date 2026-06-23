import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/ElementMoveHarness.svelte";
import { MockIntersectionObserver } from "./__fixtures__/intersection-observer-mock";
import { makeAnchor } from "./__fixtures__/resize-observer-mock";

describe("useElementMove", () => {
  beforeEach(() => {
    MockIntersectionObserver.instances = [];
    vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("arms an IntersectionObserver on mount without bumping version", () => {
    const el = makeAnchor({ top: 100, left: 0, width: 200, height: 20 });

    const { getByTestId } = render(Harness, { el });
    const out = getByTestId("element-move");

    // First refresh measures and arms, but does NOT count as a move.
    expect(out.dataset.version).toBe("0");
    expect(MockIntersectionObserver.instances).toHaveLength(1);
    expect(MockIntersectionObserver.instances[0].observed).toContain(el);
  });

  it("bumps version and re-arms (observing the element again) when it moves", () => {
    const el = makeAnchor({ top: 100, left: 0, width: 200, height: 20 });

    const { getByTestId } = render(Harness, { el });
    const out = getByTestId("element-move");
    expect(out.dataset.version).toBe("0");

    // Ratio < threshold (1) on the first update => the element moved => onMove.
    MockIntersectionObserver.instances[0].trigger(0.5);
    flushSync();

    expect(out.dataset.version).toBe("1");
    // Re-armed with a fresh observer (the old one was disconnected) that is
    // actually re-observing the element — otherwise later moves wouldn't fire.
    expect(MockIntersectionObserver.instances.length).toBeGreaterThanOrEqual(2);
    expect(MockIntersectionObserver.instances[0].disconnected).toBe(true);
    expect(MockIntersectionObserver.latest!.observed).toContain(el);
  });

  it("keeps bumping version on subsequent moves (second update re-arms at threshold 1)", () => {
    const el = makeAnchor({ top: 100, left: 0, width: 200, height: 20 });

    const { getByTestId } = render(Harness, { el });
    const out = getByTestId("element-move");

    MockIntersectionObserver.latest!.trigger(0.5); // first move
    flushSync();
    expect(out.dataset.version).toBe("1");

    MockIntersectionObserver.latest!.trigger(0.3); // moved again
    flushSync();
    expect(out.dataset.version).toBe("2");
    expect(MockIntersectionObserver.latest!.observed).toContain(el);
  });

  it("polls back via a timer when the element moves fully off-screen (ratio 0)", () => {
    vi.useFakeTimers();
    try {
      const el = makeAnchor({ top: 100, left: 0, width: 200, height: 20 });

      const { getByTestId } = render(Harness, { el });
      const out = getByTestId("element-move");
      const armedCount = MockIntersectionObserver.instances.length;

      // Ratio 0 => off-screen: no immediate move, a re-arm is scheduled.
      MockIntersectionObserver.latest!.trigger(0);
      flushSync();
      expect(out.dataset.version).toBe("0");
      expect(MockIntersectionObserver.instances.length).toBe(armedCount);

      // After the poll interval it re-arms (a fresh observer re-observes el).
      vi.advanceTimersByTime(1000);
      flushSync();
      expect(MockIntersectionObserver.instances.length).toBeGreaterThan(armedCount);
      expect(MockIntersectionObserver.latest!.observed).toContain(el);
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not arm an observer for a zero-area element", () => {
    const el = makeAnchor({ top: 0, left: 0, width: 0, height: 0 });

    render(Harness, { el });

    expect(MockIntersectionObserver.instances).toHaveLength(0);
  });

  it("leaves version at zero when the element is null", () => {
    const { getByTestId } = render(Harness, { el: null });
    const out = getByTestId("element-move");

    expect(out.dataset.version).toBe("0");
    expect(MockIntersectionObserver.instances).toHaveLength(0);
  });

  it("disconnects the observer on unmount", () => {
    const el = makeAnchor({ top: 100, left: 0, width: 200, height: 20 });

    const { unmount } = render(Harness, { el });
    const observer = MockIntersectionObserver.instances[0];
    expect(observer.disconnected).toBe(false);

    unmount();
    expect(observer.disconnected).toBe(true);
  });

  it("re-subscribes when the element prop changes", async () => {
    const elA = makeAnchor({ top: 100, left: 0, width: 200, height: 20 });
    const elB = makeAnchor({ top: 300, left: 0, width: 200, height: 20 });

    const { rerender } = render(Harness, { el: elA });
    const first = MockIntersectionObserver.instances[0];
    expect(first.observed).toContain(elA);

    await rerender({ el: elB });

    expect(first.disconnected).toBe(true);
    expect(MockIntersectionObserver.latest!.observed).toContain(elB);
  });
});
