import { describe, it, expect, vi, afterEach } from "vitest";
import { flushSync } from "svelte";
import { useSelectionPopover, type SelectionPopover } from "./useSelectionPopover.svelte";
import type { ElementSize } from "./useElementSize.svelte";

// jsdom implements getBoundingClientRect on Element but not on Range. The hook
// clones the captured range (so a per-instance stub wouldn't survive) and its
// position effect calls getBoundingClientRect on the clone, so patch the
// prototype with a zero rect for the whole suite.
Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 0, 0);

let teardown: (() => void) | null = null;

afterEach(() => {
  teardown?.();
  teardown = null;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const size: ElementSize = { width: 0, height: 0, version: 0 };

function mount(getArticle: () => HTMLElement | null, isEnabled: () => boolean): SelectionPopover {
  let popover!: SelectionPopover;
  const cleanup = $effect.root(() => {
    popover = useSelectionPopover(getArticle, size, isEnabled);
  });
  flushSync(); // run effects so the document listeners attach
  teardown = cleanup;
  return popover;
}

/** Build an <article> holding `text`, plus a Range over that text node. */
function articleWithText(text: string): { article: HTMLElement; range: Range } {
  const article = document.createElement("article");
  const p = document.createElement("p");
  p.textContent = text;
  article.appendChild(p);
  document.body.appendChild(article);
  const range = document.createRange();
  range.selectNodeContents(p.firstChild as Text);
  return { article, range };
}

/** Build an <article> with a diagram figure, plus a Range over its SVG label. */
function articleWithDiagram(): { article: HTMLElement; diagramRange: Range } {
  const article = document.createElement("article");
  article.innerHTML = `<p>before</p><figure class="diagram"><svg><text>Billing</text></svg></figure><p>after</p>`;
  document.body.appendChild(article);
  const label = article.querySelector("text")!.firstChild as Text;
  const diagramRange = document.createRange();
  diagramRange.setStart(label, 0);
  diagramRange.setEnd(label, label.data.length);
  return { article, diagramRange };
}

/** Stub window.getSelection: a non-null range is a non-collapsed selection. */
function stubSelection(range: Range | null) {
  vi.spyOn(window, "getSelection").mockReturnValue({
    isCollapsed: range === null,
    rangeCount: range ? 1 : 0,
    getRangeAt: () => range as Range,
  } as unknown as Selection);
}

function mouseup() {
  document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  flushSync();
}

describe("useSelectionPopover", () => {
  it("captures a non-collapsed selection on a document-level mouseup (released anywhere)", () => {
    const { article, range } = articleWithText("Welcome to the docs");
    const popover = mount(
      () => article,
      () => true,
    );

    stubSelection(range);
    mouseup(); // dispatched on document — the target is NOT the article

    expect(popover.pos).not.toBeNull();
  });

  it("clears a captured selection when a later one falls outside the article", () => {
    const { article, range } = articleWithText("inside article");
    const aside = document.createElement("aside");
    aside.textContent = "sidebar text";
    document.body.appendChild(aside);
    const outside = document.createRange();
    outside.selectNodeContents(aside.firstChild as Text);

    const popover = mount(
      () => article,
      () => true,
    );

    stubSelection(range);
    mouseup();
    expect(popover.pos).not.toBeNull();

    // Extending the selection out past the article boundary (still non-collapsed,
    // so selectionchange won't dismiss it) must drop the stale popover.
    stubSelection(outside);
    mouseup();
    expect(popover.pos).toBeNull();
  });

  it("ignores a collapsed selection (a plain click)", () => {
    const { article } = articleWithText("Welcome");
    const popover = mount(
      () => article,
      () => true,
    );
    stubSelection(null);
    mouseup();
    expect(popover.pos).toBeNull();
  });

  it("does not capture while comments are disabled", () => {
    const { article, range } = articleWithText("Welcome");
    const popover = mount(
      () => article,
      () => false,
    );
    stubSelection(range);
    mouseup();
    expect(popover.pos).toBeNull();
  });

  it("drops a captured selection when comments become disabled", () => {
    const { article, range } = articleWithText("Welcome");
    let enabled = $state(true);
    const popover = mount(
      () => article,
      () => enabled,
    );

    stubSelection(range);
    mouseup();
    expect(popover.pos).not.toBeNull();

    enabled = false;
    flushSync();
    expect(popover.pos).toBeNull();

    // The listener is detached too, not just the range cleared: a fresh
    // in-article selection released while disabled must not re-open the popover.
    stubSelection(range);
    mouseup();
    expect(popover.pos).toBeNull();
  });

  it("clear() drops the current selection", () => {
    const { article, range } = articleWithText("Welcome");
    const popover = mount(
      () => article,
      () => true,
    );

    stubSelection(range);
    mouseup();
    expect(popover.pos).not.toBeNull();

    popover.clear();
    flushSync();
    expect(popover.pos).toBeNull();
  });

  it("does not open the popover for a selection inside a diagram", () => {
    const { article, diagramRange } = articleWithDiagram();
    const popover = mount(
      () => article,
      () => true,
    );
    stubSelection(diagramRange);
    mouseup();
    expect(popover.pos).toBeNull();
  });

  it("dismisses on selectionchange-collapse", () => {
    const { article, range } = articleWithText("Welcome");
    const popover = mount(
      () => article,
      () => true,
    );

    stubSelection(range);
    mouseup();
    expect(popover.pos).not.toBeNull();

    stubSelection(null); // selection collapsed
    document.dispatchEvent(new Event("selectionchange"));
    flushSync();
    expect(popover.pos).toBeNull();
  });
});
