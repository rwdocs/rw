import { describe, it, expect } from "vitest";
import { rangeToSelectors, selectorsToRange } from "./anchoring";
import type { Selector } from "../types/comments";

function createContainer(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

function selectText(container: HTMLElement, text: string): Range {
  const fullText = container.textContent ?? "";
  const index = fullText.indexOf(text);
  if (index === -1) throw new Error(`"${text}" not found in container`);

  const range = document.createRange();
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let offset = 0;
  let startSet = false;

  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const len = node.textContent?.length ?? 0;

    if (!startSet && offset + len > index) {
      range.setStart(node, index - offset);
      startSet = true;
    }
    if (startSet && offset + len >= index + text.length) {
      range.setEnd(node, index + text.length - offset);
      break;
    }
    offset += len;
  }
  return range;
}

describe("rangeToSelectors", () => {
  it("returns empty array for collapsed selection", () => {
    const container = createContainer("<p>Hello world</p>");
    const range = document.createRange();
    range.setStart(container.firstChild!.firstChild!, 0);
    range.collapse(true);

    expect(rangeToSelectors(range, container)).toEqual([]);
  });

  it("creates TextQuoteSelector and TextPositionSelector", () => {
    const container = createContainer("<p>The quick brown fox jumps over the lazy dog</p>");
    const range = selectText(container, "brown fox");
    const selectors = rangeToSelectors(range, container);

    expect(selectors).toHaveLength(2);
    expect(selectors[0]).toEqual({
      type: "TextQuoteSelector",
      exact: "brown fox",
      prefix: "The quick ",
      suffix: " jumps over the lazy dog",
    });
    expect(selectors[1]).toEqual({
      type: "TextPositionSelector",
      start: 10,
      end: 19,
    });
  });

  it("handles selection at the start of the container", () => {
    const container = createContainer("<p>Hello world</p>");
    const range = selectText(container, "Hello");
    const selectors = rangeToSelectors(range, container);

    const quote = selectors.find((s) => s.type === "TextQuoteSelector")!;
    expect(quote).toMatchObject({ exact: "Hello", prefix: "" });

    const pos = selectors.find((s) => s.type === "TextPositionSelector")!;
    expect(pos).toMatchObject({ start: 0, end: 5 });
  });

  it("handles selection spanning multiple elements", () => {
    const container = createContainer("<p><b>bold</b> and <i>italic</i> text</p>");
    const range = selectText(container, "and italic");
    const selectors = rangeToSelectors(range, container);

    expect(selectors[0]).toMatchObject({
      type: "TextQuoteSelector",
      exact: "and italic",
    });
  });

  it("truncates prefix/suffix to 32 characters", () => {
    const long = "a".repeat(50);
    const container = createContainer(`<p>${long}TARGET${long}</p>`);
    const range = selectText(container, "TARGET");
    const selectors = rangeToSelectors(range, container);

    const quote = selectors.find((s) => s.type === "TextQuoteSelector")!;
    if (quote.type === "TextQuoteSelector") {
      expect(quote.prefix.length).toBe(32);
      expect(quote.suffix.length).toBe(32);
    }
  });
});

describe("selectorsToRange", () => {
  it("re-anchors using position selector when text matches", () => {
    const container = createContainer("<p>The quick brown fox</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "brown", prefix: "The quick ", suffix: " fox" },
      { type: "TextPositionSelector", start: 10, end: 15 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("brown");
    expect(result!.strategy).toBe("position");
  });

  it("falls back to quote selector when position text does not match", () => {
    const container = createContainer("<p>NEW TEXT The quick brown fox</p>");
    // Position is stale (points to wrong text), but quote should find it
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "brown", prefix: "The quick ", suffix: " fox" },
      { type: "TextPositionSelector", start: 10, end: 15 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("brown");
    expect(result!.strategy).toBe("quote");
  });

  it("works with only TextPositionSelector", () => {
    const container = createContainer("<p>Hello world</p>");
    const selectors: Selector[] = [{ type: "TextPositionSelector", start: 6, end: 11 }];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("world");
    expect(result!.strategy).toBe("position");
  });

  it("works with only TextQuoteSelector", () => {
    const container = createContainer("<p>Hello world</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "world", prefix: "Hello ", suffix: "" },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("world");
    expect(result!.strategy).toBe("quote");
  });

  it("returns null when text is not found and not similar", () => {
    const container = createContainer("<p>Hello world</p>");
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "completely unrelated phrase",
        prefix: "",
        suffix: "",
      },
      { type: "TextPositionSelector", start: 100, end: 200 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).toBeNull();
  });

  it("returns null for empty selectors array", () => {
    const container = createContainer("<p>Hello world</p>");
    expect(selectorsToRange([], container)).toBeNull();
  });

  it("disambiguates repeated text using context", () => {
    const container = createContainer("<p>foo bar baz foo bar qux</p>");
    // Target the second "foo bar" — suffix "qux" matches only the second occurrence
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "foo bar", prefix: "baz ", suffix: " qux" },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("foo bar");

    // Verify it picked the second occurrence by checking position
    const fullText = container.textContent ?? "";
    const preRange = document.createRange();
    preRange.setStart(container, 0);
    preRange.setEnd(result!.range.startContainer, result!.range.startOffset);
    const startPos = preRange.toString().length;
    expect(startPos).toBe(fullText.lastIndexOf("foo bar"));
  });

  it("handles selection spanning multiple DOM nodes", () => {
    const container = createContainer("<p><b>bold</b> and <i>italic</i></p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "bold and italic", prefix: "", suffix: "" },
      { type: "TextPositionSelector", start: 0, end: 15 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("bold and italic");
  });

  // Regression for scenario N2 in test-results/comment-stability:
  // when the document SHRINKS so the original passage moves earlier than the
  // stored position, AND a duplicate quote appears later, the resolver must
  // still pick the one whose context matches — even though the original is
  // before the position-hint window.
  it("picks the context-matching occurrence even when it is before the position hint", () => {
    const container = createContainer(
      "<p>We study the brown fox jumps deeply later on in the chapter.</p>" +
        "<p>Later note: a quick brown fox jumps for fun.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "brown fox jumps",
        prefix: "ph here for setup.We study the ", // matches the original position
        suffix: " deeply later on in the chapter.", // matches the first occurrence
      },
      { type: "TextPositionSelector", start: 46, end: 61 }, // stale — original is at 13 now
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("quote");
    const fullText = container.textContent ?? "";
    const preRange = document.createRange();
    preRange.setStart(container, 0);
    preRange.setEnd(result!.range.startContainer, result!.range.startOffset);
    const startPos = preRange.toString().length;
    // Should have picked the first occurrence (the one with matching suffix
    // " deeply later on in the chapter."), not the second one ("for fun.").
    expect(startPos).toBe(fullText.indexOf("brown fox jumps"));
  });
});

describe("fuzzy fallback", () => {
  it("re-anchors after a single character is deleted from inside the quote", () => {
    // Original quote was "brown fox jumps" — now the document reads
    // "brown fx jumps" (one char dropped). Exact match fails; fuzzy succeeds.
    const container = createContainer("<p>The quick brown fx jumps over the lazy dog.</p>");
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "brown fox jumps",
        prefix: "The quick ",
        suffix: " over the lazy dog.",
      },
      { type: "TextPositionSelector", start: 10, end: 25 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("fuzzy");
  });

  it("re-anchors after the renderer drops the space between two paragraphs", () => {
    // Scenario J2: comment was on "brown fox jumps", then a paragraph break
    // got inserted between "fox" and "jumps". The renderer concatenates
    // sibling <p> textContent with no separator, so the post-edit string is
    // "...brown foxjumps over...". Exact match fails — fuzzy should win.
    const container = createContainer("<p>The quick brown fox</p><p>jumps over the lazy dog.</p>");
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "brown fox jumps",
        prefix: "The quick ",
        suffix: " over the lazy dog.",
      },
      { type: "TextPositionSelector", start: 10, end: 25 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("fuzzy");
  });

  it("returns null instead of throwing when the quote exceeds diff-match-patch's pattern bit width", () => {
    // diff-match-patch throws "Pattern too long for this browser." when a
    // pattern longer than Match_MaxBits (default 32) isn't a verbatim match.
    // An orphaned inline comment with a long stored quote hits that branch
    // on every re-anchor pass; the exception must not bubble up and break
    // the caller's anchoring loop (PageContent relies on null to mark the
    // comment as an orphan).
    const container = createContainer("<p>totally unrelated content on this page.</p>");
    const longQuote = "a".repeat(64);
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: longQuote,
        prefix: "before",
        suffix: "after",
      },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).toBeNull();
  });

  it("orphans when the passage was rewritten beyond the similarity threshold", () => {
    // No "brown fox jumps", and the closest substring ("hens scatter feed")
    // is too far below the threshold.
    const container = createContainer(
      "<p>The rooster crows at dawn while the hens scatter and feed.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "brown fox jumps",
        prefix: "The quick ",
        suffix: " over the lazy dog.",
      },
      { type: "TextPositionSelector", start: 10, end: 25 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).toBeNull();
  });
});

describe("roundtrip", () => {
  it("rangeToSelectors -> selectorsToRange preserves the selection", () => {
    const container = createContainer(
      "<p>First paragraph with some text.</p><p>Second paragraph here.</p>",
    );
    const range = selectText(container, "some text");
    const selectors = rangeToSelectors(range, container);

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("some text");
  });

  it("roundtrip works across element boundaries", () => {
    const container = createContainer("<p>Start <code>code</code> and <em>emphasis</em> end</p>");
    const range = selectText(container, "code and emphasis");
    const selectors = rangeToSelectors(range, container);

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("code and emphasis");
  });
});
