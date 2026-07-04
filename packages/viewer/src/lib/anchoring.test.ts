import { describe, it, expect, afterEach } from "vitest";
import {
  rangeToSelectors,
  selectorsToRange,
  buildTextIndex,
  selectorsToRangeIn,
} from "./anchoring";
import type { Selector } from "../types/comments";

afterEach(() => {
  document.body.replaceChildren();
});

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

describe("buildTextIndex", () => {
  it("concatenates text content across nodes and elements", () => {
    const container = createContainer("<p>Hello <b>brave</b> world</p>");
    const index = buildTextIndex(container);
    expect(index.text).toBe("Hello brave world");
  });

  it("locate() maps an offset to the right text node and local offset", () => {
    const container = createContainer("<p>Hello <b>brave</b> world</p>");
    const index = buildTextIndex(container);
    // offset 8 falls inside "brave" (H-e-l-l-o-space=6, then b=6,r=7,a=8)
    const at = index.locate(8)!;
    expect(at.node.data).toBe("brave");
    expect(at.offset).toBe(2);
  });

  it("locate() at a node boundary returns the end of the previous node", () => {
    const container = createContainer("<p>ab</p><p>cd</p>");
    const index = buildTextIndex(container);
    // offset 2 is the boundary between "ab" (0..2) and "cd" (2..4)
    const at = index.locate(2)!;
    expect(at.node.data).toBe("ab");
    expect(at.offset).toBe(2);
  });

  it("locate() returns null for an offset past the end", () => {
    const container = createContainer("<p>abc</p>");
    const index = buildTextIndex(container);
    expect(index.locate(99)).toBeNull();
  });

  it("locate(0) maps to the start of the first text node", () => {
    // Offset 0 is the boundary the binary search must return as {firstNode, 0};
    // it relies on idx initializing to 0, so pin it down explicitly — a comment
    // whose quote starts at the very beginning of the article anchors here.
    const container = createContainer("<p>Hello <b>brave</b> world</p>");
    const at = buildTextIndex(container).locate(0)!;
    expect(at.node.data).toBe("Hello ");
    expect(at.offset).toBe(0);
  });
});

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

  it("re-anchors a long (>32 char) quote after a leading word is deleted", () => {
    // 39-char quote (longer than 32) whose leading "DOMA and " was dropped
    // from the heading. The surviving suffix still agrees, so the fuzzy
    // matcher re-anchors it. Concatenated textContent (no inter-block spaces)
    // is "Q3 plans.Target architecture of domainsWe start docs."
    const container = createContainer(
      "<p>Q3 plans.</p><h2>Target architecture of domains</h2><p>We start docs.</p>",
    );
    const exact = "DOMA and Target architecture of domains"; // 39 chars, absent verbatim
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact,
        prefix: "Q3 plans.",
        suffix: "We start docs.",
      },
      { type: "TextPositionSelector", start: 9, end: 9 + exact.length },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("fuzzy");
    // Landed on the real heading, not just some fuzzy region.
    expect(result!.range.toString()).toContain("Target architecture of domains");
  });

  it("re-anchors after a word inside the quote is substituted", () => {
    // "draft" was edited to "final" (a 5-char substitution) inside a 25-char
    // quote (~0.2 edit ratio); the surviving context re-anchors it.
    const container = createContainer(
      "<p>By Q3 we deliver a final target architecture for review.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "draft target architecture",
        prefix: "we deliver a ",
        suffix: " for review.",
      },
      { type: "TextPositionSelector", start: 18, end: 43 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("fuzzy");
    // Landed on the edited passage, not the unrelated text around it.
    expect(result!.range.toString()).toContain("target architecture");
  });

  it("re-anchors when the stored position hint has drifted far from the passage", () => {
    // The passage now sits ~300 chars past its stored position (text was
    // inserted above it). Re-anchoring must tolerate a drifted position hint —
    // the hint is only a tiebreaker, not a hard distance constraint.
    const container = createContainer(
      "<p>" + "x".repeat(300) + " and later a final target architecture summary here.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "draft target architecture summary",
        prefix: "later a ",
        suffix: " here.",
      },
      { type: "TextPositionSelector", start: 5, end: 38 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("fuzzy");
    // Anchored to the drifted passage far from the stored position.
    expect(result!.range.toString()).toContain("target architecture summary");
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

  it("orphans a long quote with no similar passage instead of anchoring at random", () => {
    // A 64-char quote (longer than 32) absent from a short, unrelated page has
    // no candidate within maxErrors = ceil(64*0.3) = 20 (aligning it to ~38
    // chars of unrelated text costs far more than 20 edits), so the matcher
    // returns no matches and the comment orphans instead of anchoring at random.
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

  it("orphans instead of wrong-anchoring when a more-similar passage shadows the edited original", () => {
    // The Myers matcher returns only its lowest-error region. Here the original
    // passage was edited ("report card" -> "summary card") while a different
    // sentence ("...budget report follows") is strictly closer to the stored
    // quote, so it shadows the original and is the only region returned. Its
    // surrounding context does not match the stored prefix/suffix, so the
    // confidence gate rejects it: the comment orphans to the timeline rather
    // than jumping to the wrong sentence.
    const container = createContainer(
      "<p>In H1 the quarterly budget summary card shipped. Elsewhere: monthly the quarterly budget report follows next.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "the quarterly budget report card",
        prefix: "In H1 ",
        suffix: " shipped. Else",
      },
      { type: "TextPositionSelector", start: 6, end: 38 },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
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

describe("low-confidence demotion", () => {
  it("orphans a short lone quote when no surviving occurrence has strong context", () => {
    // Original doc: "abc - def - xyz", comment on the first "-".
    // After edit, doc is "abc def - xyz" (one "-" left, at offset 8).
    // The lone "-" has weak prefix/suffix match against the stored context
    // (only the bordering spaces agree). Must NOT anchor inline.
    const container = createContainer("<p>abc def - xyz</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "-", prefix: "abc ", suffix: " def - xyz" },
      { type: "TextPositionSelector", start: 4, end: 5 },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  it("orphans a short common word with weak context after the original passage is gone", () => {
    // Common-token quote ("TODO") that survives elsewhere on the page but
    // with totally unrelated context. Must not anchor inline.
    const container = createContainer("<p>morning standup TODO xyz</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "TODO", prefix: "finished ", suffix: " item" },
      { type: "TextPositionSelector", start: 8, end: 12 },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  it("keeps a single-char quote anchored when both sides have strong context", () => {
    // The same short-quote shape, but with surrounding context that DOES agree —
    // this is a legitimate anchor and must NOT be demoted.
    const container = createContainer("<p>foo - bar</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "-", prefix: "foo ", suffix: " bar" },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("quote");
    expect(result!.range.toString()).toBe("-");
  });

  it("keeps a long quote anchored when only one side of context matches", () => {
    // 30-char quote, prefix matches verbatim, suffix is totally different.
    // For long quotes, one strong side is enough to stay anchored as quote.
    const longExact = "this is a thirty char chunk!!!"; // longer than the short-quote threshold
    const container = createContainer(`<p>aa before bb ${longExact} totally rewritten</p>`);
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: longExact,
        prefix: "aa before bb ",
        suffix: " original context here that no longer exists",
      },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("quote");
    expect(result!.range.toString()).toBe(longExact);
  });

  it("keeps a quote anchored when both stored sides are empty (boundary selection)", () => {
    // Selection touched the start/end of the article — stored prefix and
    // suffix are both empty. Confidence can't be assessed from context;
    // accept the match.
    const container = createContainer("<p>single match somewhere</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "single match", prefix: "", suffix: "" },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("quote");
    expect(result!.range.toString()).toBe("single match");
  });

  it("returns null when stored TextQuoteSelector.exact is empty", () => {
    // A scripted/external caller could persist a selector with an empty
    // `exact`. Without a guard, quoteBestOccurrence's `text.indexOf("", n)`
    // loop never terminates because indexOf("") clamps at text.length
    // instead of returning -1.
    const container = createContainer("<p>hello world</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "", prefix: "", suffix: "" },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  it("orphans a long quote when one stored side is empty and the other side disagrees", () => {
    // Long quote (>= SHORT_QUOTE_LEN). Empty prefix (boundary selection at
    // article start), recorded suffix that no longer matches in the live doc.
    // Empty side must NOT short-circuit the confidence gate to true.
    const longExact = "this is a long passage"; // 22 chars
    const container = createContainer(`<p>random preamble ${longExact} totally different tail</p>`);
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: longExact,
        prefix: "",
        suffix: " original context that no longer exists",
      },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  it("orphans a short quote with empty stored prefix when the only recorded side disagrees", () => {
    // Short quote, empty prefix (boundary selection), recorded suffix that
    // matches a wrong occurrence. Without the empty-side fix, suffixOk passes
    // and the comment anchors to the wrong "ok".
    const container = createContainer("<p>ok suddenly unrelated</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "ok", prefix: "", suffix: " then proceed" },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  it("anchors a verbatim match even when stored context is shorter than the confidence floor", () => {
    // Stored prefix is only 1 char (boundary or tiny-article selection); a
    // verbatim re-anchor must succeed because the maximum achievable score
    // (= recorded length) is what we should expect.
    const container = createContainer("<p>X-Y</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "-", prefix: "X", suffix: "Y" },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.strategy).toBe("quote");
    expect(result!.range.toString()).toBe("-");
  });

  it("orphans a short quote when position validates exact but stored context disagrees", () => {
    // Original doc: "abc - def - xyz", comment on first "-" at offset 4.
    // User reorders: "abc - xyz - def". First "-" is STILL at offset 4 and
    // still equals stored exact "-", but the stored context ("abc ", " def - xyz")
    // doesn't match the new surroundings. Position branch must not short-circuit
    // the confidence floor — fall through to quote search, which then orphans.
    const container = createContainer("<p>abc - xyz - def</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "-", prefix: "abc ", suffix: " def - xyz" },
      { type: "TextPositionSelector", start: 4, end: 5 },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });
});

describe("unique short-quote relaxation", () => {
  // A unique short heading ("Metrics", 7 < SHORT_QUOTE_LEN) whose SUFFIX changed:
  // a paragraph was inserted after it. Prefix ("Report" H1) intact, suffix broken,
  // position stale. The heading text stream is "Report" + "Metrics" + <para>, so
  // "Metrics" sits at offset 6.
  it("anchors a unique short exact when only the prefix side still agrees", () => {
    const container = createContainer(
      "<h1>Report</h1><h2>Metrics</h2><p>An inserted intro paragraph now leads.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "Metrics",
        prefix: "Report",
        suffix: "First bullet under the heading ",
      },
      // Stale position: at record time "Metrics" sat at 6–13; that still holds
      // here, but the suffix no longer matches so the position path's confidence
      // check fails and it falls through to the quote path.
      { type: "TextPositionSelector", start: 6, end: 13 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("Metrics");
    expect(result!.strategy).toBe("quote");
  });

  // Symmetric: unique short exact whose PREFIX changed (H1 gone), suffix intact.
  it("anchors a unique short exact when only the suffix side still agrees", () => {
    const container = createContainer(
      "<h2>Metrics</h2><p>First bullet under the heading follows.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "Metrics",
        prefix: "Report",
        suffix: "First bullet under the heading ",
      },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("Metrics");
    expect(result!.strategy).toBe("quote");
  });

  // Proves the fix lives in the quote path, independent of position validity:
  // no position selector at all, unique short exact, only the prefix agrees.
  it("anchors a unique short exact via the quote path with no position selector", () => {
    const container = createContainer(
      "<h1>Report</h1><h2>Metrics</h2><p>Totally different text below.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "Metrics",
        prefix: "Report",
        suffix: "First bullet under the heading ",
      },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("Metrics");
    expect(result!.strategy).toBe("quote");
  });

  // The position fast-path still handles a unique short exact directly (as
  // "position", not falling through to the quote path) when both recorded
  // sides still agree — the relaxation didn't disturb the happy path.
  it("anchors a unique short exact via the position path when both sides still agree", () => {
    const container = createContainer(
      "<h1>Report</h1><h2>Metrics</h2><p>First bullet under the heading follows.</p>",
    );
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "Metrics",
        prefix: "Report",
        suffix: "First bullet under the heading ",
      },
      { type: "TextPositionSelector", start: 6, end: 13 },
    ];

    const result = selectorsToRange(selectors, container);
    expect(result).not.toBeNull();
    expect(result!.range.toString()).toBe("Metrics");
    expect(result!.strategy).toBe("position");
  });

  // A self-overlapping short exact ("aa" in "aaa") is counted as more than one
  // occurrence (the scan steps by i+1), so it stays NON-unique and on the
  // strict both-sides gate. Guards against a future indexOf-step change silently
  // making overlapping short exacts eligible for the one-side relaxation.
  it("treats a self-overlapping short exact as non-unique (strict gate)", () => {
    const container = createContainer("<p>Baaa</p>");
    const selectors: Selector[] = [
      // "aa" occurs at offsets 1 and 2 (overlapping) → non-unique. Prefix "B"
      // matches the first occurrence, suffix does not — one strong side only,
      // which must NOT be enough for a non-unique short exact.
      { type: "TextQuoteSelector", exact: "aa", prefix: "B", suffix: "zzzz" },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  // Protection preserved: multiple occurrences of a short exact with only
  // one-side context must still orphan (both-sides rule stays for ambiguity).
  it("still orphans a short exact with multiple occurrences and one-side context", () => {
    const container = createContainer("<p>alpha TODO beta</p><p>gamma TODO delta</p>");
    const selectors: Selector[] = [
      // "TODO" (4 < 8) appears twice; suffix " beta" strongly matches the first
      // occurrence but the prefix does not, and it is not unique — must orphan.
      { type: "TextQuoteSelector", exact: "TODO", prefix: "zzzz ", suffix: " beta" },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
  });

  // Didn't over-relax: a unique short exact whose BOTH sides changed still orphans.
  it("still orphans a unique short exact when both context sides changed", () => {
    const container = createContainer("<p>xxxx Metrics yyyy</p>");
    const selectors: Selector[] = [
      {
        type: "TextQuoteSelector",
        exact: "Metrics",
        prefix: "Report",
        suffix: "First bullet under the heading ",
      },
    ];

    expect(selectorsToRange(selectors, container)).toBeNull();
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

describe("selectorsToRangeIn", () => {
  it("matches the container-based selectorsToRange on a position anchor", () => {
    const container = createContainer("<p>The quick brown fox</p>");
    const selectors: Selector[] = [
      { type: "TextQuoteSelector", exact: "brown", prefix: "The quick ", suffix: " fox" },
      { type: "TextPositionSelector", start: 10, end: 15 },
    ];
    const viaContainer = selectorsToRange(selectors, container);
    const viaIndex = selectorsToRangeIn(selectors, buildTextIndex(container));
    expect(viaIndex!.range.toString()).toBe(viaContainer!.range.toString());
    expect(viaIndex!.strategy).toBe(viaContainer!.strategy);
  });

  it("resolves many comments from one index and wraps each correctly", async () => {
    const container = createContainer("<p>alpha beta gamma delta epsilon zeta</p>");
    const index = buildTextIndex(container);
    const words = ["alpha", "gamma", "epsilon"];
    const ranges = words.map(
      (w) =>
        selectorsToRangeIn(
          [{ type: "TextQuoteSelector", exact: w, prefix: "", suffix: "" }],
          index,
        )!.range,
    );
    // Wrap each precomputed range one by one and verify the DOM wrappers are correct.
    const { wrapRange } = await import("./comments/highlight");
    ranges.forEach((r, i) => wrapRange(r, { commentId: `c${i}`, strategy: "quote" }));
    expect(container.querySelectorAll("rw-annotation")).toHaveLength(3);
    expect([...container.querySelectorAll("rw-annotation")].map((e) => e.textContent)).toEqual(
      words,
    );
  });
});

describe("buildTextIndex diagram exclusion", () => {
  const HTML = `<p>before</p><figure class="diagram"><svg><text>Billing</text></svg></figure><p>after</p>`;

  it("omits diagram text from the concatenated stream", () => {
    const idx = buildTextIndex(createContainer(HTML));
    expect(idx.text).toBe("beforeafter");
    expect(idx.text).not.toContain("Billing");
  });

  it("offsetOf maps a prose boundary to its filtered offset (skipping the diagram)", () => {
    const c = createContainer(HTML);
    const afterP = c.querySelectorAll("p")[1].firstChild as Text;
    const idx = buildTextIndex(c);
    // "after" begins right after "before" (6 chars) in the filtered stream.
    expect(idx.offsetOf(afterP, 0)).toBe(6);
    expect(idx.offsetOf(afterP, 5)).toBe(11);
  });

  it("offsetOf and locate round-trip on prose after a diagram", () => {
    const c = createContainer(HTML);
    const afterP = c.querySelectorAll("p")[1].firstChild as Text;
    const idx = buildTextIndex(c);
    const off = idx.offsetOf(afterP, 2)!;
    const loc = idx.locate(off)!;
    expect(loc.node).toBe(afterP);
    expect(loc.offset).toBe(2);
  });

  it("offsetOf handles an element-boundary container after a diagram", () => {
    // Boundary containers are Elements (child-index offsets), not text nodes,
    // when a selection starts/ends at a tag boundary. Filtered stream is
    // "beforebold".
    const c = createContainer(
      `<p>before</p><figure class="diagram"><svg><text>Billing</text></svg></figure><p><b>bo</b>ld</p>`,
    );
    const p2 = c.querySelectorAll("p")[1];
    const idx = buildTextIndex(c);
    expect(idx.offsetOf(p2, 0)).toBe(6); // before <b>: past "before", diagram skipped
    expect(idx.offsetOf(p2, 1)).toBe(8); // between <b>bo</b> and "ld": 6 + 2
  });
});

describe("rangeToSelectors diagram exclusion", () => {
  it("computes prose offsets unaffected by a diagram above the selection", () => {
    const c = createContainer(
      `<p>before</p><figure class="diagram"><svg><text>Billing</text></svg></figure><p>target here</p>`,
    );
    const p = c.querySelectorAll("p")[1].firstChild as Text;
    const range = document.createRange();
    range.setStart(p, 0);
    range.setEnd(p, 6); // "target"
    const selectors = rangeToSelectors(range, c);
    const pos = selectors.find((s) => s.type === "TextPositionSelector");
    const quote = selectors.find((s) => s.type === "TextQuoteSelector");
    // Filtered stream is "beforetarget here"; "target" starts at 6, not 13.
    expect(pos).toEqual({ type: "TextPositionSelector", start: 6, end: 12 });
    expect(quote).toMatchObject({ type: "TextQuoteSelector", exact: "target", prefix: "before" });
  });

  it("returns [] for a selection entirely inside a diagram", () => {
    const c = createContainer(
      `<p>before</p><figure class="diagram"><svg><text>Billing</text></svg></figure>`,
    );
    const label = c.querySelector("text")!.firstChild as Text;
    const range = document.createRange();
    range.setStart(label, 0);
    range.setEnd(label, label.data.length);
    expect(rangeToSelectors(range, c)).toEqual([]);
  });
});
