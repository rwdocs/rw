import { describe, it, expect, afterEach } from "vitest";
import { reconcileHighlights, type DesiredComment } from "./reconcile";
import type { Selector } from "../../types/comments";

afterEach(() => {
  document.body.replaceChildren();
});

function createContainer(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

function quote(exact: string): Selector[] {
  return [{ type: "TextQuoteSelector", exact, prefix: "", suffix: "" }];
}

function desired(id: string, exact: string, parentId: string | null = null): DesiredComment {
  return { id, selectors: quote(exact), parentId };
}

function wrappedIds(container: HTMLElement): string[] {
  return [...container.querySelectorAll("rw-annotation")].map(
    (e) => e.getAttribute("data-comment-id")!,
  );
}

describe("reconcileHighlights — fresh document", () => {
  it("wraps every desired comment when the DOM has no wrappers", () => {
    const container = createContainer("<p>alpha beta gamma</p>");
    const result = reconcileHighlights(
      container,
      [desired("a", "alpha"), desired("g", "gamma")],
      null,
    );
    expect(new Set(wrappedIds(container))).toEqual(new Set(["a", "g"]));
    expect([...result.ranges.keys()].sort()).toEqual(["a", "g"]);
    expect(result.order).toEqual(["a", "g"]); // document order
    expect(result.orphanIds.size).toBe(0);
  });

  it("orphans a top-level comment whose quote is absent", () => {
    const container = createContainer("<p>alpha beta</p>");
    const result = reconcileHighlights(container, [desired("x", "nonexistent")], null);
    expect(result.orphanIds.has("x")).toBe(true);
    expect(result.ranges.has("x")).toBe(false);
    expect(wrappedIds(container)).toHaveLength(0);
  });

  it("does not orphan a reply (parentId set) that fails to anchor", () => {
    const container = createContainer("<p>alpha beta</p>");
    const result = reconcileHighlights(container, [desired("r", "nope", "parent")], null);
    expect(result.orphanIds.has("r")).toBe(false);
  });

  it("orphans a top-level comment whose parentId is undefined (real API shape)", () => {
    // The API `Comment.parentId` is optional (`string | undefined`), so a
    // top-level comment arrives with parentId undefined, not null. The orphan
    // check must treat undefined as top-level.
    const container = createContainer("<p>alpha beta</p>");
    const result = reconcileHighlights(
      container,
      [{ id: "u", selectors: quote("nonexistent"), parentId: undefined }],
      null,
    );
    expect(result.orphanIds.has("u")).toBe(true);
  });
});

describe("reconcileHighlights — incremental", () => {
  it("removes a no-longer-desired comment's wrapper but leaves others' DOM nodes identical", () => {
    const container = createContainer("<p>alpha beta gamma</p>");
    reconcileHighlights(container, [desired("a", "alpha"), desired("g", "gamma")], null);
    const gammaNodeBefore = container.querySelector('rw-annotation[data-comment-id="g"]');

    // Resolve "a" → drop it from desired.
    const result = reconcileHighlights(container, [desired("g", "gamma")], null);
    expect(wrappedIds(container)).toEqual(["g"]);
    const gammaNodeAfter = container.querySelector('rw-annotation[data-comment-id="g"]');
    expect(gammaNodeAfter).toBe(gammaNodeBefore); // untouched DOM node identity
    expect(result.ranges.has("a")).toBe(false);
    // The unwrap-only path still rebuilds the survivor's range from the final
    // DOM (normalize after unwrap merges text nodes) — it must stay valid.
    expect(result.ranges.get("g")!.toString()).toBe("gamma");
  });

  it("adds a newly desired comment without re-wrapping the existing one", () => {
    const container = createContainer("<p>alpha beta gamma</p>");
    reconcileHighlights(container, [desired("a", "alpha")], null);
    const alphaBefore = container.querySelector('rw-annotation[data-comment-id="a"]');

    const result = reconcileHighlights(
      container,
      [desired("a", "alpha"), desired("g", "gamma")],
      null,
    );
    expect(new Set(wrappedIds(container))).toEqual(new Set(["a", "g"]));
    expect(container.querySelector('rw-annotation[data-comment-id="a"]')).toBe(alphaBefore);
    expect(result.order).toEqual(["a", "g"]);
  });
});

describe("reconcileHighlights — stored ranges stay valid", () => {
  it("keeps non-collapsed ranges for two comments sharing one text node", () => {
    const container = createContainer("<p>alpha beta gamma delta</p>");
    const result = reconcileHighlights(
      container,
      [desired("a", "alpha"), desired("g", "gamma")],
      null,
    );
    expect(result.ranges.get("a")!.toString()).toBe("alpha");
    expect(result.ranges.get("g")!.toString()).toBe("gamma");
  });

  it("keeps a non-collapsed range for an existing comment when a new one is added into the same text node", () => {
    const container = createContainer("<p>alpha beta gamma delta</p>");
    reconcileHighlights(container, [desired("a", "alpha")], null);
    const result = reconcileHighlights(
      container,
      [desired("a", "alpha"), desired("g", "gamma")],
      null,
    );
    expect(result.ranges.get("a")!.toString()).toBe("alpha");
    expect(result.ranges.get("g")!.toString()).toBe("gamma");
  });
});

describe("reconcileHighlights — selection", () => {
  it("reports touchesSelection=false when an unwrap does not overlap the selection", () => {
    const container = createContainer("<p>alpha beta gamma</p>");
    reconcileHighlights(container, [desired("a", "alpha"), desired("g", "gamma")], null);

    // Select "gamma".
    const sel = document.createRange();
    const gammaWrapper = container.querySelector('rw-annotation[data-comment-id="g"]')!;
    sel.selectNodeContents(gammaWrapper);

    // Resolve "alpha" (does not overlap the "gamma" selection).
    const result = reconcileHighlights(container, [desired("g", "gamma")], sel);
    expect(result.touchesSelection).toBe(false);
  });

  it("reports touchesSelection=true when an unwrap overlaps the selection", () => {
    const container = createContainer("<p>alpha beta gamma</p>");
    reconcileHighlights(container, [desired("a", "alpha")], null);
    const aWrapper = container.querySelector('rw-annotation[data-comment-id="a"]')!;
    const sel = document.createRange();
    sel.selectNodeContents(aWrapper);

    const result = reconcileHighlights(container, [], sel);
    expect(result.touchesSelection).toBe(true);
  });

  it("reports touchesSelection=true when an added wrap overlaps the selection", () => {
    const container = createContainer("<p>alpha beta gamma</p>");
    // Select "alpha" before it is wrapped.
    const sel = document.createRange();
    const p = container.querySelector("p")!.firstChild as Text;
    sel.setStart(p, 0);
    sel.setEnd(p, 5); // "alpha"
    const result = reconcileHighlights(container, [desired("a", "alpha")], sel);
    expect(result.touchesSelection).toBe(true);
  });
});

describe("reconcileHighlights — overlapping comments", () => {
  // Regression for the e2e "overlapping comments render with nested wrappers"
  // failure: when two overlapping comments are wrapped in ONE pass (initial
  // load, or a re-rendered article), wrapping the first splits the shared text
  // node and would collapse the second's pre-computed range. The wrap loop must
  // re-resolve each against the live DOM so the second still nests.
  it("nests two overlapping comments wrapped in a single pass", () => {
    const c = createContainer("<p>Welcome to the test documentation site.</p>");
    reconcileHighlights(
      c,
      [
        desired("outer", "Welcome to the test documentation"),
        desired("inner", "to the test documentation site"),
      ],
      null,
    );
    // Both wrapped, and a nested wrapper exists over the overlap region.
    expect(new Set(wrappedIds(c))).toEqual(new Set(["outer", "inner"]));
    expect(c.querySelector("rw-annotation rw-annotation")).not.toBeNull();
  });

  it("nests an overlapping comment added in a later pass over an existing one", () => {
    const c = createContainer("<p>Welcome to the test documentation site.</p>");
    reconcileHighlights(c, [desired("outer", "Welcome to the test documentation")], null);
    const outerBefore = c.querySelector('rw-annotation[data-comment-id="outer"]');
    reconcileHighlights(
      c,
      [
        desired("outer", "Welcome to the test documentation"),
        desired("inner", "to the test documentation site"),
      ],
      null,
    );
    // The pre-existing outer wrapper is not torn down, and inner nests.
    expect(c.querySelector('rw-annotation[data-comment-id="outer"]')).toBe(outerBefore);
    expect(c.querySelector("rw-annotation rw-annotation")).not.toBeNull();
  });
});
