import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/ActiveHeadingHarness.svelte";
import { MockResizeObserver } from "./__fixtures__/resize-observer-mock";
import type { ActiveHeading } from "./useActiveHeading.svelte";

// The resolver draws its highlight line at 20% of the viewport. Pinning the
// viewport turns that into a fixed pixel value, so a heading's position can be
// written as its distance above or below the line rather than a bare number.
const VIEWPORT_HEIGHT = 1000;
const LINE = VIEWPORT_HEIGHT * 0.2;

function rectOf(top: number, height: number, width = 600): DOMRect {
  return {
    top,
    bottom: top + height,
    height,
    left: 0,
    right: width,
    width,
    x: 0,
    y: top,
    toJSON: () => ({}),
  };
}

/** The all-zero rect an element in a non-rendered subtree reports. */
function hideHeading(el: HTMLElement): void {
  el.getBoundingClientRect = () => rectOf(0, 0, 0);
}

/** Append an <h2> whose viewport-relative top is fixed at `top`. */
function placeHeading(id: string, top: number, height = 40): HTMLElement {
  const el = document.createElement("h2");
  el.id = id;
  el.getBoundingClientRect = () => rectOf(top, height);
  document.body.appendChild(el);
  return el;
}

/**
 * Re-parent the headings into a scrollable container, as a host app that owns
 * the viewport does. jsdom reports 0 for both scroll metrics, so they are
 * defined outright.
 */
function hostScroller({
  scrollTop,
  scrollHeight,
  clientHeight,
  top = 0,
}: {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  /** Where the scrollport sits in the window, e.g. below a host header. */
  top?: number;
}): HTMLElement {
  const box = document.createElement("div");
  box.style.overflowY = "auto";
  // headings → <article> → scrollport, so the container and the scrollport are
  // distinct elements exactly as they are in the app.
  const article = document.createElement("article");
  for (const heading of [...document.body.querySelectorAll("h2")]) article.appendChild(heading);
  box.appendChild(article);
  document.body.appendChild(box);
  Object.defineProperty(box, "scrollHeight", { value: scrollHeight, configurable: true });
  Object.defineProperty(box, "clientHeight", { value: clientHeight, configurable: true });
  box.getBoundingClientRect = () => rectOf(top, clientHeight);
  box.scrollTop = scrollTop;
  return box;
}

/** Grow a scroller's content so an ancestor that fitted now overflows. */
function overflow(box: HTMLElement, scrollHeight: number): void {
  Object.defineProperty(box, "scrollHeight", { value: scrollHeight, configurable: true });
}

/** Move an already-placed heading, as a reflow would. */
function moveHeading(el: HTMLElement, top: number, height = 40): void {
  el.getBoundingClientRect = () => rectOf(top, height);
}

function setViewport({
  scrollY = 0,
  scrollHeight = 5000,
  innerHeight = VIEWPORT_HEIGHT,
}: { scrollY?: number; scrollHeight?: number; innerHeight?: number } = {}): void {
  window.innerHeight = innerHeight;
  // Unlike innerHeight, scrollHeight is a getter with no setter.
  Object.defineProperty(document.documentElement, "scrollHeight", {
    value: scrollHeight,
    configurable: true,
  });
  window.scrollY = scrollY;
}

/** Scroll-listener removals that would actually detach a capture-phase listener. */
function scrollRemovals(spy: { mock: { calls: unknown[][] } }): unknown[][] {
  return spy.mock.calls.filter(
    ([type, , options]) =>
      type === "scroll" && (options as AddEventListenerOptions | undefined)?.capture === true,
  );
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

/**
 * Fire a scroll and wait out the rAF throttle. Dispatched on `document`, not
 * `window`, so only a capture-phase window listener sees it — the spelling an
 * ancestor scroll container needs.
 */
async function scrollTo(y: number): Promise<void> {
  window.scrollY = y;
  document.dispatchEvent(new Event("scroll"));
  await nextFrame();
}

/**
 * A scroll in an ancestor container: the capture-phase listener sees the event,
 * but `window.scrollY` never moves because the window is not the scroller.
 */
async function ancestorScroll(): Promise<void> {
  document.dispatchEvent(new Event("scroll"));
  await nextFrame();
}

async function resize(innerHeight: number): Promise<void> {
  window.innerHeight = innerHeight;
  window.dispatchEvent(new Event("resize"));
  await nextFrame();
}

describe("useActiveHeading", () => {
  let captured: ActiveHeading | null = null;
  const captureInit = (handle: ActiveHeading) => {
    captured = handle;
  };

  beforeEach(() => {
    captured = null;
    MockResizeObserver.instances = [];
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
    setViewport();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  it("attaches no listeners for an empty heading list", () => {
    const addSpy = vi.spyOn(window, "addEventListener");

    const { getByTestId } = render(Harness, { headingIds: [], onInit: captureInit });

    expect(addSpy.mock.calls.map(([type]) => type)).not.toContain("scroll");
    expect(getByTestId("active-heading").dataset.active).toBe("");
    expect(captured?.activeId).toBeNull();
  });

  it("activates the first heading when the reader is above every heading", () => {
    placeHeading("intro", LINE + 200);
    placeHeading("usage", LINE + 600);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    expect(getByTestId("active-heading").dataset.active).toBe("intro");
  });

  it("activates the last heading above the line", async () => {
    placeHeading("intro", -500);
    // Exactly on the line, which counts as passed.
    placeHeading("usage", LINE);
    placeHeading("api", LINE + 400);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await scrollTo(500);

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("prefers the lower of two adjacent headings above the line", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE - 100);
    placeHeading("usage-detail", LINE - 60);
    placeHeading("api", LINE + 400);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "usage-detail", "api"],
      onInit: captureInit,
    });

    await scrollTo(500);

    expect(getByTestId("active-heading").dataset.active).toBe("usage-detail");
  });

  it("skips ids that have no element in the DOM", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "missing", "usage"],
      onInit: captureInit,
    });

    await scrollTo(500);

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("leaves activeId null when no id has an element", async () => {
    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    await scrollTo(500);

    expect(getByTestId("active-heading").dataset.active).toBe("");
    expect(captured?.activeId).toBeNull();
  });

  it("picks up headings that moved without any list change", async () => {
    placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE - 100);
    placeHeading("api", LINE + 400);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    await scrollTo(500);
    expect(getByTestId("active-heading").dataset.active).toBe("usage");

    // A late web-font swap pushes the section down past the line.
    moveHeading(usage, LINE + 100);
    await scrollTo(501);

    expect(getByTestId("active-heading").dataset.active).toBe("intro");
  });

  it("recomputes on resize with no scroll", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE + 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    await scrollTo(500);
    expect(getByTestId("active-heading").dataset.active).toBe("intro");

    // A taller viewport drops the line to 400, which "usage" is now above.
    await resize(2000);

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("resets to the first heading when a new document reuses the same ids", async () => {
    placeHeading("overview", -500);
    const config = placeHeading("configuration", LINE - 100);
    const examples = placeHeading("examples", LINE + 400);

    const { getByTestId, rerender } = render(Harness, {
      headingIds: ["overview", "configuration", "examples"],
      onInit: captureInit,
    });
    await scrollTo(500);
    expect(getByTestId("active-heading").dataset.active).toBe("configuration");

    // The sibling document: identical outline, reader back at the top.
    const overview = document.getElementById("overview")!;
    moveHeading(overview, LINE + 200);
    moveHeading(config, LINE + 700);
    moveHeading(examples, LINE + 1200);
    window.scrollY = 0;
    await rerender({
      headingIds: ["overview", "configuration", "examples"],
      onInit: captureInit,
    });

    expect(getByTestId("active-heading").dataset.active).toBe("overview");
  });

  it("activates the last heading at the bottom of a scrollable document", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", -100);
    placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    // maxScroll is 5000 - 1000 = 4000.
    await scrollTo(4000);

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("does not activate the last heading just short of the bottom", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", -100);
    placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await scrollTo(3900);

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("tolerates fractional rounding at the bottom", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", -100);
    placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await scrollTo(3999);

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("keeps the first heading at the bottom of a barely scrollable document", async () => {
    // The page bottoms out while the first heading is still below the line, so
    // only checking "passed none" ahead of the bottom rule keeps the outline
    // off the last entry.
    setViewport({ scrollHeight: 1050 });
    placeHeading("intro", LINE + 200);
    placeHeading("usage", LINE + 600);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    await scrollTo(50);

    expect(getByTestId("active-heading").dataset.active).toBe("intro");
  });

  it("activates the last heading at the bottom of a host-owned scroller", async () => {
    // The window cannot scroll at all here; the container the host gave us can.
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 100);
    placeHeading("api", LINE + 300);
    hostScroller({ scrollTop: 4000, scrollHeight: 5000, clientHeight: 1000 });

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await ancestorScroll();

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("draws the line inside a scrollport that starts below the window's top", async () => {
    // A host header pushes the scrollport to y=300; its own line is therefore
    // at 300 + 500*0.2 = 400, not the window's 200.
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    placeHeading("intro", 250);
    placeHeading("usage", 380);
    placeHeading("api", 600);
    hostScroller({ scrollTop: 500, scrollHeight: 5000, clientHeight: 500, top: 300 });

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await ancestorScroll();

    // Against the window's line only "intro" has passed; against the
    // scrollport's, the reader is on "usage".
    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("notices an ancestor that becomes scrollable after the headings are listed", async () => {
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 100);
    placeHeading("api", LINE + 300);
    // Fits at first, so nothing scrolls it yet.
    const box = hostScroller({ scrollTop: 0, scrollHeight: 1000, clientHeight: 1000 });

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    // A late diagram makes the host's container scrollable, and the reader
    // takes it to the end.
    overflow(box, 5000);
    box.scrollTop = 4000;
    await ancestorScroll();

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("recomputes when the host resizes the scrollport", async () => {
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    placeHeading("intro", 250);
    placeHeading("usage", 380);
    const box = hostScroller({ scrollTop: 500, scrollHeight: 5000, clientHeight: 500, top: 300 });

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");
    // Line at 300 + 500*0.2 = 400, so the reader has passed "usage".
    await ancestorScroll();
    expect(out.dataset.active).toBe("usage");

    // The scrollport is watched in its own right: the headings' container is a
    // different element, and a host shrinking its panel — a drawer, a split
    // pane — touches neither that nor the window.
    expect(MockResizeObserver.instances.at(-1)!.observed).toContain(box);

    // Shrunk to 200, the line rises to 340 and "usage" is below it again.
    Object.defineProperty(box, "clientHeight", { value: 200, configurable: true });
    box.getBoundingClientRect = () => rectOf(300, 200);
    MockResizeObserver.instances.at(-1)!.trigger();
    await nextFrame();

    expect(out.dataset.active).toBe("intro");
  });

  it("needs no recomputation when the scrollport only moves", async () => {
    // A translation carries the headings with it, so the line and every rect
    // shift by the same amount and the answer cannot change — which is why
    // ResizeObserver not reporting position costs nothing here.
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    const intro = placeHeading("intro", 250);
    const usage = placeHeading("usage", 380);
    const box = hostScroller({ scrollTop: 500, scrollHeight: 5000, clientHeight: 500, top: 300 });

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    await ancestorScroll();
    expect(getByTestId("active-heading").dataset.active).toBe("usage");

    const shift = 220;
    box.getBoundingClientRect = () => rectOf(300 + shift, 500);
    moveHeading(intro, 250 + shift);
    moveHeading(usage, 380 + shift);
    await ancestorScroll();

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("notices an ancestor the host switches to scrolling at a breakpoint", async () => {
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 100);
    placeHeading("api", LINE + 300);
    // Laid out as a plain block: nothing scrolls it when the outline is listed.
    const box = hostScroller({ scrollTop: 0, scrollHeight: 5000, clientHeight: 1000 });
    box.style.overflowY = "visible";

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    // A media query — or a panel opening — makes it the scrollport, and the
    // reader takes it to the end. No heading changed, so nothing re-listed.
    box.style.overflowY = "auto";
    box.scrollTop = 4000;
    box.dispatchEvent(new Event("scroll", { bubbles: false }));
    await nextFrame();

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("observes a scrollport it only discovers later", async () => {
    setViewport({ scrollHeight: VIEWPORT_HEIGHT });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 100);
    const box = hostScroller({ scrollTop: 0, scrollHeight: 5000, clientHeight: 1000 });
    box.style.overflowY = "visible";

    render(Harness, { headingIds: ["intro", "usage"], onInit: captureInit });
    expect(MockResizeObserver.instances.at(-1)!.observed).not.toContain(box);

    box.style.overflowY = "auto";
    box.dispatchEvent(new Event("scroll", { bubbles: false }));
    await nextFrame();

    // Discovered by that scroll — and it must be watched from now on, or a
    // later resize of the panel moves the line with nothing to notice.
    expect(MockResizeObserver.instances.at(-1)!.observed).toContain(box);
  });

  it("does not re-read styles when an unrelated element scrolls", () => {
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 100);
    hostScroller({ scrollTop: 0, scrollHeight: 5000, clientHeight: 1000 });
    // A scroller of its own with no headings in it — the nav sidebar, a wide
    // code block. Re-discovery walks ancestors reading computed styles, which
    // is a style recalc; it must not run for these.
    const sidebar = document.createElement("div");
    sidebar.style.overflowY = "auto";
    document.body.appendChild(sidebar);

    render(Harness, { headingIds: ["intro", "usage"], onInit: captureInit });

    const styles = vi.spyOn(window, "getComputedStyle");
    sidebar.dispatchEvent(new Event("scroll", { bubbles: false }));

    expect(styles).not.toHaveBeenCalled();
  });

  it("ignores an ancestor that could scroll but does not", async () => {
    // `overflow-y: auto` alone does not make an element the scroller. Mistaking
    // one for it would read a zero scroll range and silence the bottom rule on
    // a page the window scrolls perfectly well.
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 100);
    placeHeading("api", LINE + 300);
    hostScroller({ scrollTop: 0, scrollHeight: 1000, clientHeight: 1000 });

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await scrollTo(4000);

    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("keeps the first heading when the page overflows the viewport by a pixel", async () => {
    // maxScroll of 1 is inside the bottom epsilon, so scrollY 0 would satisfy
    // "at the bottom" if the epsilon were not itself guarded.
    setViewport({ scrollHeight: VIEWPORT_HEIGHT + 1 });
    placeHeading("intro", LINE - 100);
    placeHeading("usage", LINE + 200);
    placeHeading("api", LINE + 500);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    await scrollTo(0);

    expect(getByTestId("active-heading").dataset.active).toBe("intro");
  });

  it("skips headings in a subtree that is not rendered", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE - 100);
    const hidden = placeHeading("hidden-tab", 0);
    hideHeading(hidden);
    placeHeading("api", LINE + 400);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "hidden-tab", "api"],
      onInit: captureInit,
    });

    await scrollTo(500);

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("counts a rendered heading whose box has no height", async () => {
    // An empty heading lays out with a real width and a real position; only an
    // all-zero rect means "not rendered".
    placeHeading("intro", -500);
    const empty = placeHeading("empty", LINE - 100);
    empty.getBoundingClientRect = () => rectOf(LINE - 100, 0, 600);
    placeHeading("api", LINE + 400);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "empty", "api"],
      onInit: captureInit,
    });

    await scrollTo(500);

    expect(getByTestId("active-heading").dataset.active).toBe("empty");
  });

  it("leaves the highlight alone when nothing is rendered at all", async () => {
    placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    await scrollTo(500);
    expect(getByTestId("active-heading").dataset.active).toBe("usage");

    // The article is swapped for a loading skeleton while its outline stays up.
    hideHeading(document.getElementById("intro")!);
    hideHeading(usage);
    await scrollTo(501);

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("seeds a new document even while a suppression is armed", async () => {
    placeHeading("overview", -500);
    const config = placeHeading("configuration", LINE - 100);

    const { getByTestId, rerender } = render(Harness, {
      headingIds: ["overview", "configuration"],
      onInit: captureInit,
    });
    await scrollTo(500);
    expect(getByTestId("active-heading").dataset.active).toBe("configuration");

    // Click a ToC entry, then navigate before the scroll settles. The frame
    // below is where suppression commits to waiting for `scrollend`; without
    // it the rAF would release on the spot and the test would not enter the
    // state it exists to cover.
    captured!.choose("configuration", { scrollTo: () => {} });
    window.scrollY = 4242;
    await nextFrame();

    const overview = document.getElementById("overview")!;
    moveHeading(overview, LINE + 200);
    moveHeading(config, LINE + 700);
    window.scrollY = 0;
    await rerender({
      headingIds: ["overview", "configuration"],
      onInit: captureInit,
    });

    expect(getByTestId("active-heading").dataset.active).toBe("overview");
  });

  it("recomputes when the page reflows without a scroll or resize", async () => {
    placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE + 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    await scrollTo(500);
    expect(getByTestId("active-heading").dataset.active).toBe("intro");

    // A late diagram above "usage" pulls it up past the line.
    moveHeading(usage, 150);
    MockResizeObserver.instances.at(-1)!.trigger();
    await nextFrame();

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("observes the element holding the headings, not the viewport-pinned body", () => {
    // `app.css` pins `html, body, #app` to `height: 100%`, so body's box is the
    // viewport: observing it would never report the content growth this exists
    // to catch.
    const article = document.createElement("article");
    document.body.appendChild(article);
    const intro = placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE - 100);
    article.append(intro, usage);

    render(Harness, { headingIds: ["intro", "usage"], onInit: captureInit });

    expect(MockResizeObserver.instances.at(-1)!.observed).toEqual([article]);
  });

  it("detaches the scrollend listener when the timeout releases instead", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE - 100);

    render(Harness, { headingIds: ["intro", "usage"], onInit: captureInit });
    const usage = document.getElementById("usage")!;

    const add = vi.spyOn(window, "addEventListener");
    const remove = vi.spyOn(window, "removeEventListener");
    // Matched by handler identity, not by count: a pending fallback from an
    // earlier test can fire inside this one's window.
    const handlers = (spy: typeof add) =>
      spy.mock.calls.filter(([type]) => type === "scrollend").map(([, handler]) => handler);

    // Two clicks whose scrolls never report `scrollend` — the Safari case the
    // fallback exists for. Each must clean up after itself.
    for (const top of [LINE - 200, LINE - 300]) {
      captured!.choose("usage", { scrollTo: () => moveHeading(usage, top) });
      await nextFrame();
      await new Promise((resolve) => setTimeout(resolve, 550));
    }

    const added = handlers(add);
    const removed = new Set(handlers(remove));
    expect(added).toHaveLength(2);
    expect(added.filter((handler) => !removed.has(handler))).toEqual([]);
  });

  it("releases suppression on the timeout when scrollend never fires", async () => {
    placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    // The scroll moves the chosen heading, so suppression waits for
    // `scrollend` — which some browsers never fire, and which is skipped when a
    // scroll is interrupted.
    captured!.choose("usage", { scrollTo: () => moveHeading(usage, LINE - 300) });
    await nextFrame();

    moveHeading(usage, LINE + 700);
    await scrollTo(100);
    expect(out.dataset.active).toBe("usage");

    await new Promise((resolve) => setTimeout(resolve, 550));

    // The reader scrolls once suppression has lapsed.
    moveHeading(usage, LINE + 900);
    await scrollTo(101);

    expect(out.dataset.active).toBe("intro");
  });

  it("holds a chosen entry through a reflow after a programmatic scroll", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", -100);
    const api = placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    // The click-to-anchor flow: choose an entry, let the browser carry the
    // reader to it, and only then let the page reflow underneath them.
    captured!.choose("usage", { scrollTo: () => {} });
    window.scrollY = 4000;
    await nextFrame();
    window.dispatchEvent(new Event("scrollend"));

    // Geometry disagrees at the landing position, so only a pin anchored there
    // keeps the chosen entry.
    moveHeading(api, LINE - 100);
    window.dispatchEvent(new Event("resize"));
    await nextFrame();

    expect(out.dataset.active).toBe("usage");
  });

  it("holds a chosen entry through a reflow at the same scroll position", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    const usage = placeHeading("usage", -100);
    placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    // The reader clicks "usage" and the jump bottoms the document out, where
    // geometry alone would resolve to the final heading.
    window.scrollY = 4000;
    captured!.choose("usage");
    flushSync();

    moveHeading(usage, -100);
    MockResizeObserver.instances.at(-1)!.trigger();
    await nextFrame();
    expect(out.dataset.active).toBe("usage");

    window.dispatchEvent(new Event("resize"));
    await nextFrame();

    expect(out.dataset.active).toBe("usage");
  });

  it("yields a chosen entry once the reader scrolls away", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", -100);
    const api = placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    window.scrollY = 4000;
    captured!.choose("usage");
    flushSync();

    // Geometry now disagrees: "api" has passed the line, so only the pin keeps
    // "usage" current.
    moveHeading(api, LINE - 100);
    window.dispatchEvent(new Event("resize"));
    await nextFrame();
    expect(out.dataset.active).toBe("usage");

    // The reader scrolls: every heading moves, the pinned one included.
    moveHeading(document.getElementById("usage")!, LINE - 700);
    moveHeading(api, LINE - 500);
    await scrollTo(3900);

    expect(out.dataset.active).toBe("api");
  });

  it("keeps a chosen entry through a scroll that did not move it", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 300);
    placeHeading("api", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    window.scrollY = 4000;
    captured!.choose("usage");
    flushSync();

    // A programmatic jump leaves trailing scroll events behind; nothing moved,
    // so they must not hand the entry back to geometry.
    await ancestorScroll();

    expect(out.dataset.active).toBe("usage");
  });

  it("re-measures the chosen entry once the programmatic scroll settles", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    const usage = placeHeading("usage", LINE + 600);
    const api = placeHeading("api", LINE + 800);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.choose("usage", { scrollTo: () => {} });
    // The jump carries "usage" up to the line; both headings move with it.
    window.scrollY = 4000;
    moveHeading(usage, LINE - 100);
    moveHeading(api, LINE + 100);
    await nextFrame();
    window.dispatchEvent(new Event("scrollend"));

    // A trailing scroll at the resting position: the pin was re-measured there,
    // so nothing has moved since and the entry stays chosen.
    await ancestorScroll();

    expect(out.dataset.active).toBe("usage");
  });

  it("yields a chosen entry when an ancestor container scrolls", async () => {
    // Embedded in a host that owns the scroller: window.scrollY stays put, so a
    // pin keyed to it would never release and freeze the outline for good.
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    placeHeading("usage", LINE - 300);
    const api = placeHeading("api", LINE + 300);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.choose("usage");
    flushSync();
    expect(out.dataset.active).toBe("usage");

    // The reader scrolls the host container: every heading rises, the window
    // stays where it was because it is not the scroller.
    moveHeading(document.getElementById("usage")!, LINE - 700);
    moveHeading(api, LINE - 100);
    await ancestorScroll();

    expect(out.dataset.active).toBe("api");
  });

  it("survives a scroll another component starts a frame later", async () => {
    // Deep link: Layout chooses the entry, PageContent scrolls to it on the
    // next frame. That scroll must not read as the reader scrolling away.
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    const usage = placeHeading("usage", LINE + 600);
    const api = placeHeading("api", LINE + 800);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.choose("usage", { awaitScroll: true });
    flushSync();
    expect(out.dataset.active).toBe("usage");

    // The deferred jump bottoms the page out, where geometry answers "api".
    await nextFrame();
    window.scrollY = 4000;
    moveHeading(usage, LINE - 100);
    moveHeading(api, LINE + 100);
    await scrollTo(4000);

    expect(out.dataset.active).toBe("usage");
  });

  it("does not let a superseded choice release the current one", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    const usage = placeHeading("usage", LINE + 600);
    const api = placeHeading("api", LINE + 800);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    // Two deep links in quick succession — the second arrives just before the
    // first's 500ms fallback would fire.
    captured!.choose("api", { awaitScroll: true });
    await new Promise((resolve) => setTimeout(resolve, 450));
    captured!.choose("usage", { awaitScroll: true });
    flushSync();
    await new Promise((resolve) => setTimeout(resolve, 120));

    // The first choice's fallback has now come and gone. The second's scroll
    // arrives after it, and must still be covered.
    window.scrollY = 4000;
    moveHeading(usage, LINE - 100);
    moveHeading(api, LINE + 100);
    await scrollTo(4000);

    expect(out.dataset.active).toBe("usage");
  });

  it("does not let a superseded choice's pending frame release the current one", async () => {
    setViewport({ scrollHeight: 5000 });
    placeHeading("intro", -3000);
    const usage = placeHeading("usage", LINE + 600);
    const api = placeHeading("api", LINE + 800);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    // A click whose scroll moves nothing — its frame will want to release —
    // immediately superseded by a deep link whose scroll is still to come.
    captured!.choose("api", { scrollTo: () => {} });
    captured!.choose("usage", { awaitScroll: true });
    flushSync();
    await nextFrame();

    window.scrollY = 4000;
    moveHeading(usage, LINE - 100);
    moveHeading(api, LINE + 100);
    await scrollTo(4000);

    expect(out.dataset.active).toBe("usage");
  });

  it("does not carry a chosen entry into the next document", async () => {
    placeHeading("overview", -500);
    const config = placeHeading("configuration", LINE - 100);

    const { getByTestId, rerender } = render(Harness, {
      headingIds: ["overview", "configuration"],
      onInit: captureInit,
    });
    await scrollTo(500);

    captured!.choose("configuration");
    flushSync();

    // The sibling document renders at the same scroll offset — the browser
    // restores it — so only clearing the pin on a list change can free the
    // outline to answer for the new page.
    moveHeading(document.getElementById("overview")!, 400);
    moveHeading(config, LINE + 700);
    await rerender({ headingIds: ["overview", "configuration"], onInit: captureInit });

    expect(getByTestId("active-heading").dataset.active).toBe("overview");
  });

  it("lets a chosen entry override the resolved heading", () => {
    placeHeading("intro", LINE + 200);
    placeHeading("usage", LINE + 600);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });

    captured!.choose("usage");
    flushSync();

    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("ignores scroll updates while the chosen scroll is in flight", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.choose("usage", { scrollTo: () => {} });
    flushSync();
    expect(out.dataset.active).toBe("usage");

    // Move "usage" below the line so an unsuppressed resolve would answer "intro".
    moveHeading(document.getElementById("usage")!, 900);
    // Simulate a scroll so the rAF fallback path keeps suppression armed.
    window.scrollY = 42;
    await scrollTo(43);

    expect(out.dataset.active).toBe("usage");
  });

  it("releases suppression when scrollend fires so updates resume", async () => {
    placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    captured!.choose("usage", { scrollTo: () => {} });
    window.scrollY = 99;

    await nextFrame();
    window.dispatchEvent(new Event("scrollend"));

    moveHeading(usage, LINE + 700);
    await scrollTo(100);

    expect(out.dataset.active).toBe("intro");
  });

  it("releases suppression immediately when no scroll actually happened", async () => {
    placeHeading("intro", -500);
    const usage = placeHeading("usage", LINE - 100);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage"],
      onInit: captureInit,
    });
    const out = getByTestId("active-heading");

    const scrollBefore = window.scrollY;
    captured!.choose("usage", { scrollTo: () => {} });
    window.scrollY = scrollBefore;

    await nextFrame();

    moveHeading(usage, LINE + 700);
    await scrollTo(scrollBefore + 1);

    expect(out.dataset.active).toBe("intro");
  });

  it("re-subscribes when the heading list changes", async () => {
    placeHeading("intro", -500);
    placeHeading("usage", LINE - 100);
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { rerender } = render(Harness, { headingIds: ["intro"], onInit: captureInit });

    await rerender({ headingIds: ["intro", "usage"], onInit: captureInit });

    expect(scrollRemovals(removeSpy)).toHaveLength(1);
  });

  it("removes its listeners on unmount", () => {
    placeHeading("intro", -500);
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { unmount } = render(Harness, { headingIds: ["intro"], onInit: captureInit });
    expect(scrollRemovals(removeSpy)).toHaveLength(0);

    unmount();

    // The capture flag has to match the registration or removeEventListener
    // silently no-ops, leaking a listener — and a stale `ids` closure — per
    // navigation.
    expect(scrollRemovals(removeSpy)).toHaveLength(1);
    expect(removeSpy.mock.calls.map(([type]) => type)).toContain("resize");
    expect(MockResizeObserver.instances.at(-1)!.disconnected).toBe(true);
  });

  it("hands a chosen heading in a collapsed tab panel back to geometry", async () => {
    const intro = placeHeading("intro", LINE - 100);
    const collapsed = placeHeading("hidden-tab", LINE + 400);
    const api = placeHeading("api", LINE + 800);
    hideHeading(collapsed);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "hidden-tab", "api"],
      onInit: captureInit,
    });

    // scrollIntoView on a heading that is not rendered moves nothing.
    captured!.choose("hidden-tab", { scrollTo: () => {} });
    flushSync();
    expect(getByTestId("active-heading").dataset.active).toBe("hidden-tab");

    await nextFrame();

    moveHeading(intro, LINE - 900);
    moveHeading(api, LINE - 100);
    await scrollTo(800);
    flushSync();

    // An all-zero rect read as position 0 matches every later measurement, so
    // the entry would hold for the rest of the document.
    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });

  it("frees the resolver when a bare choice supersedes a scrolling one", async () => {
    const intro = placeHeading("intro", LINE - 100);
    const usage = placeHeading("usage", LINE + 400);
    const api = placeHeading("api", LINE + 800);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    captured!.choose("usage", { scrollTo: () => moveHeading(usage, LINE - 50) });
    // Supersedes it before the scroll settles, and arms no release of its own.
    captured!.choose("api");
    flushSync();

    await nextFrame();

    moveHeading(intro, LINE - 900);
    moveHeading(usage, LINE - 100);
    moveHeading(api, LINE + 300);
    await scrollTo(800);
    flushSync();

    // Inheriting the abandoned choice's suppression would silence the resolver
    // with its timer and listener already torn down.
    expect(getByTestId("active-heading").dataset.active).toBe("usage");
  });

  it("releases on a scrollend that fires before the measuring frame", async () => {
    const intro = placeHeading("intro", LINE - 100);
    const usage = placeHeading("usage", LINE + 400);
    const api = placeHeading("api", LINE + 800);

    const { getByTestId } = render(Harness, {
      headingIds: ["intro", "usage", "api"],
      onInit: captureInit,
    });

    captured!.choose("usage", {
      scrollTo: () => {
        // An instant scroll: the scroll steps dispatch `scrollend` ahead of the
        // animation-frame callbacks, so a listener armed in the frame after the
        // scroll has already missed it.
        moveHeading(usage, LINE - 50);
        window.dispatchEvent(new Event("scrollend"));
      },
    });
    flushSync();
    expect(getByTestId("active-heading").dataset.active).toBe("usage");

    await nextFrame();

    moveHeading(intro, LINE - 900);
    moveHeading(usage, LINE - 500);
    moveHeading(api, LINE - 100);
    await scrollTo(800);
    flushSync();

    // Released by the event, not by the 500ms fallback, which has not run.
    expect(getByTestId("active-heading").dataset.active).toBe("api");
  });
});
