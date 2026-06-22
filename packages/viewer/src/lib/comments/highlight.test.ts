import { describe, it, expect, afterEach } from "vitest";
import { unwrapAll, wrapRange, unwrapComment } from "./highlight";
import { rangeToSelectors } from "$lib/anchoring";

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

describe("unwrapAll", () => {
  it("is a no-op on a container with no wrappers", () => {
    const container = createContainer("<p>Hello world</p>");
    const before = container.innerHTML;
    unwrapAll(container);
    expect(container.innerHTML).toBe(before);
  });

  it("removes a single rw-annotation and restores text", () => {
    const container = createContainer(
      '<p>Hello <rw-annotation data-comment-id="a">world</rw-annotation>!</p>',
    );
    unwrapAll(container);
    expect(container.innerHTML).toBe("<p>Hello world!</p>");
    expect(container.querySelectorAll("rw-annotation")).toHaveLength(0);
  });

  it("removes nested rw-annotation elements", () => {
    const container = createContainer(
      '<p><rw-annotation data-comment-id="a">foo <rw-annotation data-comment-id="b">bar</rw-annotation> baz</rw-annotation></p>',
    );
    unwrapAll(container);
    expect(container.innerHTML).toBe("<p>foo bar baz</p>");
  });

  it("merges adjacent text nodes via normalize()", () => {
    const container = createContainer(
      '<p>Hello <rw-annotation data-comment-id="a">world</rw-annotation>!</p>',
    );
    unwrapAll(container);
    const p = container.querySelector("p")!;
    expect(p.childNodes.length).toBe(1);
    expect(p.firstChild?.nodeType).toBe(Node.TEXT_NODE);
  });
});

describe("wrapRange — single text node", () => {
  it("wraps the targeted substring with one rw-annotation", () => {
    const container = createContainer("<p>The quick brown fox</p>");
    const range = selectText(container, "brown");
    const wrappers = wrapRange(range, { commentId: "a", strategy: "position" });

    expect(wrappers).toHaveLength(1);
    expect(wrappers[0].tagName.toLowerCase()).toBe("rw-annotation");
    expect(wrappers[0].textContent).toBe("brown");
    expect(wrappers[0].getAttribute("data-comment-id")).toBe("a");
    expect(wrappers[0].getAttribute("data-strategy")).toBe("position");
    expect(container.innerHTML).toBe(
      '<p>The quick <rw-annotation data-comment-id="a" data-strategy="position">brown</rw-annotation> fox</p>',
    );
  });

  it("returns an empty array for a collapsed range", () => {
    const container = createContainer("<p>Hello</p>");
    const range = document.createRange();
    range.setStart(container.firstChild!.firstChild!, 2);
    range.collapse(true);

    expect(wrapRange(range, { commentId: "a", strategy: "position" })).toHaveLength(0);
    expect(container.innerHTML).toBe("<p>Hello</p>");
  });
});

describe("wrapRange — crosses element boundaries", () => {
  it("range crossing an inline tag produces one wrapper per text-node span", () => {
    const container = createContainer("<p>Hello <em>world</em> friend</p>");
    const range = selectText(container, "lo wor");
    const wrappers = wrapRange(range, { commentId: "x", strategy: "quote" });

    expect(wrappers).toHaveLength(2);
    expect(wrappers[0].textContent).toBe("lo ");
    expect(wrappers[1].textContent).toBe("wor");
    expect(container.innerHTML).toBe(
      '<p>Hel<rw-annotation data-comment-id="x" data-strategy="quote">lo </rw-annotation><em><rw-annotation data-comment-id="x" data-strategy="quote">wor</rw-annotation>ld</em> friend</p>',
    );
    // Second wrapper sits *inside* the <em> element — text styling preserved.
    expect(wrappers[1].parentElement?.tagName.toLowerCase()).toBe("em");
  });

  it("range spanning two paragraphs produces one wrapper per paragraph", () => {
    const container = createContainer("<p>foo bar</p><p>baz qux</p>");
    const range = selectText(container, "bar"); // first paragraph
    const rangeWide = document.createRange();
    rangeWide.setStart(range.startContainer, range.startOffset);
    // Find "baz" in the second paragraph
    const p2 = container.querySelectorAll("p")[1];
    const p2Text = p2.firstChild as Text;
    rangeWide.setEnd(p2Text, 3); // up to "baz"

    const wrappers = wrapRange(rangeWide, { commentId: "y", strategy: "position" });
    expect(wrappers).toHaveLength(2);
    expect(wrappers[0].parentElement?.tagName.toLowerCase()).toBe("p");
    expect(wrappers[1].parentElement?.tagName.toLowerCase()).toBe("p");
    expect(wrappers[0].textContent).toBe("bar");
    expect(wrappers[1].textContent).toBe("baz");
  });

  it("skips whitespace-only text-node spans", () => {
    const container = createContainer("<p>a</p>\n<p>b</p>");
    const range = document.createRange();
    range.setStart(container.firstChild!.firstChild!, 0);
    range.setEnd(container.lastChild!.firstChild!, 1);
    const wrappers = wrapRange(range, { commentId: "z", strategy: "position" });
    expect(wrappers).toHaveLength(2);
    expect(wrappers.every((w) => w.textContent && w.textContent.trim().length > 0)).toBe(true);
  });
});

describe("wrapRange — overlapping ranges", () => {
  it("wrapping a second range inside an existing wrapper nests cleanly", () => {
    const container = createContainer("<p>The quick brown fox jumps over the lazy dog</p>");

    // First comment: "quick brown fox"
    const r1 = selectText(container, "quick brown fox");
    wrapRange(r1, { commentId: "a", strategy: "position" });

    // Second comment: "brown fox jumps" — overlaps with the first
    const r2 = selectText(container, "brown fox jumps");
    const w2 = wrapRange(r2, { commentId: "b", strategy: "position" });

    // The text inside both ranges ("brown fox") should be wrapped by BOTH
    // comments. The outer wrapper is comment "a"; the inner is comment "b".
    const innerB = container.querySelector(
      'rw-annotation[data-comment-id="b"] rw-annotation[data-comment-id="a"], rw-annotation[data-comment-id="a"] rw-annotation[data-comment-id="b"]',
    );
    expect(innerB).not.toBeNull();
    expect(innerB!.textContent).toBe("brown fox");

    // textContent of the whole container must be unchanged
    expect(container.textContent).toBe("The quick brown fox jumps over the lazy dog");

    // Comment b produces wrappers for "brown fox" (inside a) and " jumps" (outside)
    expect(w2.map((el) => el.textContent).join("|")).toBe("brown fox| jumps");
  });
});

describe("wrapRange + rangeToSelectors interop", () => {
  it("creating a fresh selection crossing an existing wrapper yields the same selectors as on un-wrapped DOM", () => {
    const html = "<p>The quick brown fox jumps over the lazy dog</p>";

    // Baseline: selectors on un-wrapped DOM for "fox jumps over"
    const baseline = createContainer(html);
    const baselineRange = selectText(baseline, "fox jumps over");
    const baselineSelectors = rangeToSelectors(baselineRange, baseline);

    // With a pre-existing wrapper covering "brown fox jumps"
    const wrapped = createContainer(html);
    wrapRange(selectText(wrapped, "brown fox jumps"), {
      commentId: "existing",
      strategy: "position",
    });
    const newRange = selectText(wrapped, "fox jumps over");
    const newSelectors = rangeToSelectors(newRange, wrapped);

    expect(newSelectors).toEqual(baselineSelectors);
  });
});

describe("unwrapComment", () => {
  it("removes only the target id's wrappers and leaves siblings intact", () => {
    const container = createContainer(
      '<p><rw-annotation data-comment-id="a" data-strategy="position">foo</rw-annotation> ' +
        '<rw-annotation data-comment-id="b" data-strategy="position">bar</rw-annotation></p>',
    );
    unwrapComment(container, "a");
    expect(container.querySelectorAll('rw-annotation[data-comment-id="a"]')).toHaveLength(0);
    expect(container.querySelectorAll('rw-annotation[data-comment-id="b"]')).toHaveLength(1);
    expect(container.textContent).toBe("foo bar");
  });

  it("leaves a nested/overlapping wrapper of a different id intact", () => {
    const container = createContainer(
      '<p><rw-annotation data-comment-id="a" data-strategy="position">foo ' +
        '<rw-annotation data-comment-id="b" data-strategy="position">bar</rw-annotation></rw-annotation></p>',
    );
    unwrapComment(container, "a");
    expect(container.querySelectorAll('rw-annotation[data-comment-id="a"]')).toHaveLength(0);
    const b = container.querySelector('rw-annotation[data-comment-id="b"]');
    expect(b).not.toBeNull();
    expect(b!.textContent).toBe("bar");
    expect(container.textContent).toBe("foo bar");
  });

  it("normalizes locally so the unwrapped text becomes a single text node", () => {
    const container = createContainer(
      '<p>Hello <rw-annotation data-comment-id="a">world</rw-annotation>!</p>',
    );
    unwrapComment(container, "a");
    const p = container.querySelector("p")!;
    expect(p.childNodes.length).toBe(1);
    expect(p.firstChild?.nodeType).toBe(Node.TEXT_NODE);
  });

  it("is a no-op for an unknown id", () => {
    const container = createContainer(
      '<p><rw-annotation data-comment-id="a">foo</rw-annotation></p>',
    );
    const before = container.innerHTML;
    unwrapComment(container, "zzz");
    expect(container.innerHTML).toBe(before);
  });
});

describe("unwrapAll + wrapRange round trip", () => {
  it("re-wrapping after unwrapAll reproduces the same DOM", () => {
    const html = "<p>The quick brown fox jumps over the lazy dog</p>";
    const a = createContainer(html);
    const b = createContainer(html);

    const wrapAttrs = { commentId: "c", strategy: "position" as const };
    wrapRange(selectText(a, "brown fox"), wrapAttrs);

    wrapRange(selectText(b, "brown fox"), wrapAttrs);
    unwrapAll(b);
    wrapRange(selectText(b, "brown fox"), wrapAttrs);

    expect(b.innerHTML).toBe(a.innerHTML);
  });
});
