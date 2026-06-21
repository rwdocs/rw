import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/ActiveHeadingHarness.svelte";
import type { ActiveHeading } from "./useActiveHeading.svelte";

// jsdom does not implement IntersectionObserver, and even if it did we need
// programmatic control over the callback to simulate viewport changes.
class MockIntersectionObserver {
  static instances: MockIntersectionObserver[] = [];
  callback: IntersectionObserverCallback;
  observed: Element[] = [];
  disconnected = false;

  constructor(cb: IntersectionObserverCallback) {
    this.callback = cb;
    MockIntersectionObserver.instances.push(this);
  }

  observe(el: Element) {
    this.observed.push(el);
  }

  unobserve() {}

  disconnect() {
    this.disconnected = true;
    this.observed = [];
  }

  trigger(entries: Array<{ id: string; isIntersecting: boolean }>) {
    const observerEntries = entries.map(({ id, isIntersecting }) => {
      const target = this.observed.find((el) => el.id === id);
      if (!target) throw new Error(`No observed element with id "${id}"`);
      return { target, isIntersecting } as unknown as IntersectionObserverEntry;
    });
    this.callback(observerEntries, this as unknown as IntersectionObserver);
  }
}

function makeHeading(id: string): HTMLElement {
  const el = document.createElement("h2");
  el.id = id;
  document.body.appendChild(el);
  return el;
}

function latestObserver(): MockIntersectionObserver {
  const observer = MockIntersectionObserver.instances.at(-1);
  if (!observer) throw new Error("No IntersectionObserver was created");
  return observer;
}

describe("useActiveHeading", () => {
  let captured: ActiveHeading | null = null;
  const captureInit = (handle: ActiveHeading) => {
    captured = handle;
  };

  beforeEach(() => {
    MockIntersectionObserver.instances = [];
    captured = null;
    vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  it("does not create an observer for an empty heading list", () => {
    const { getByTestId } = render(Harness, { headingIds: [], onInit: captureInit });
    const out = getByTestId("active-heading");

    expect(MockIntersectionObserver.instances).toHaveLength(0);
    expect(out.dataset.active).toBe("");
    expect(captured?.activeId).toBeNull();
  });

  it("seeds activeId with the first heading on mount", () => {
    makeHeading("intro");
    makeHeading("usage");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    expect(getByTestId("active-heading").dataset.active).toBe("intro");
    expect(captured?.activeId).toBe("intro");
  });

  it("observes every heading id that is present in the DOM", () => {
    makeHeading("intro");
    makeHeading("usage");

    render(Harness, { headingIds: ["intro", "missing", "usage"], onInit: captureInit });

    const observer = latestObserver();
    const observedIds = observer.observed.map((el) => el.id);
    expect(observedIds).toEqual(["intro", "usage"]);
  });

  it("updates activeId when an observed heading becomes intersecting", () => {
    makeHeading("intro");
    makeHeading("usage");
    makeHeading("api");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");
    expect(out.dataset.active).toBe("intro");

    latestObserver().trigger([{ id: "usage", isIntersecting: true }]);
    flushSync();

    expect(out.dataset.active).toBe("usage");
  });

  it("prefers the topmost heading in list order when several are visible", () => {
    makeHeading("intro");
    makeHeading("usage");
    makeHeading("api");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    latestObserver().trigger([
      { id: "api", isIntersecting: true },
      { id: "usage", isIntersecting: true },
    ]);
    flushSync();

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("keeps the last active heading when no heading is currently visible", () => {
    makeHeading("intro");
    makeHeading("usage");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    const observer = latestObserver();
    observer.trigger([{ id: "usage", isIntersecting: true }]);
    flushSync();
    expect(getByTestId("active-heading").dataset.active).toBe("usage");

    observer.trigger([{ id: "usage", isIntersecting: false }]);
    flushSync();

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("ignores observer updates while suppression is active", async () => {
    makeHeading("intro");
    makeHeading("usage");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.setActiveId("usage");
    captured!.suppressUntilScrollEnd();
    flushSync();
    expect(out.dataset.active).toBe("usage");

    // Simulate a scroll so the fallback path kicks in; otherwise the rAF
    // callback releases suppression synchronously on the next frame.
    window.scrollY = 42;

    latestObserver().trigger([{ id: "intro", isIntersecting: true }]);
    flushSync();

    expect(out.dataset.active).toBe("usage");
  });

  it("releases suppression when scrollend fires so observer updates resume", async () => {
    makeHeading("intro");
    makeHeading("usage");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.setActiveId("usage");
    captured!.suppressUntilScrollEnd();
    window.scrollY = 99;

    // Wait for rAF to register the scrollend listener.
    await new Promise((resolve) => requestAnimationFrame(resolve));
    window.dispatchEvent(new Event("scrollend"));

    latestObserver().trigger([{ id: "intro", isIntersecting: true }]);
    flushSync();

    expect(out.dataset.active).toBe("intro");
  });

  it("releases suppression immediately when no scroll actually happened", async () => {
    makeHeading("intro");
    makeHeading("usage");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    const scrollBefore = window.scrollY;
    captured!.setActiveId("usage");
    captured!.suppressUntilScrollEnd();
    // scrollY stays equal → rAF path sees no movement, releases on the spot.
    window.scrollY = scrollBefore;

    await new Promise((resolve) => requestAnimationFrame(resolve));

    latestObserver().trigger([{ id: "intro", isIntersecting: true }]);
    flushSync();

    expect(out.dataset.active).toBe("intro");
  });

  it("lets setActiveId override the current active heading", () => {
    makeHeading("intro");
    makeHeading("usage");

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    captured!.setActiveId("usage");
    flushSync();

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("keeps activeId when the heading list changes and the id is still present", async () => {
    makeHeading("intro");
    makeHeading("usage");
    makeHeading("api");

    const { getByTestId, rerender } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    captured!.setActiveId("usage");
    flushSync();
    expect(getByTestId("active-heading").dataset.active).toBe("usage");

    await rerender({ headingIds: ["usage", "api"], onInit: captureInit });

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("resets activeId to the first heading when the list no longer contains it", async () => {
    makeHeading("intro");
    makeHeading("usage");
    makeHeading("api");

    const { getByTestId, rerender } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    captured!.setActiveId("usage");
    flushSync();

    await rerender({ headingIds: ["api"], onInit: captureInit });

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("disconnects the previous observer when the heading list changes", async () => {
    makeHeading("intro");
    makeHeading("usage");

    const { rerender } = render(Harness, {
      headingIds: ["intro"],
      onInit: captureInit,
    });
    const first = latestObserver();
    expect(first.disconnected).toBe(false);

    await rerender({ headingIds: ["intro", "usage"], onInit: captureInit });

    expect(first.disconnected).toBe(true);
    expect(MockIntersectionObserver.instances).toHaveLength(2);
  });

  it("disconnects the observer on unmount", () => {
    makeHeading("intro");

    const { unmount } = render(Harness, {
      headingIds: ["intro"],
      onInit: captureInit,
    });
    const observer = latestObserver();
    expect(observer.disconnected).toBe(false);

    unmount();

    expect(observer.disconnected).toBe(true);
  });
});
