import { describe, it, expect, afterEach } from "vitest";
import { rangeToSelectors } from "./anchoring";
import { findReadingAnchor, textRangeFrom } from "./readingAnchor";

afterEach(() => {
  document.body.replaceChildren();
});

function article(html: string): HTMLElement {
  const el = document.createElement("article");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

describe("textRangeFrom", () => {
  it("spans text nodes from the point", () => {
    const root = article("<p>Alpha <em>beta</em> gamma</p>");
    const alpha = root.querySelector("p")!.firstChild!;

    expect(textRangeFrom(root, alpha, 2, 10)?.toString()).toBe("pha beta g");
  });

  it("stops at the end of the text", () => {
    const root = article("<p>Short.</p>");

    expect(textRangeFrom(root, root, 0, 30)?.toString()).toBe("Short.");
  });

  it("starts after a diagram the point is on", () => {
    const root = article(
      '<figure class="diagram"><svg><text>Label</text></svg></figure><p>After the diagram.</p>',
    );

    expect(textRangeFrom(root, root.querySelector("figure")!, 0, 9)?.toString()).toBe("After the");
  });

  it("is null for an article with no text", () => {
    expect(textRangeFrom(article("<p></p>"), document.body, 0)).toBeNull();
  });
});

describe("findReadingAnchor", () => {
  const before = "<p>One line here.</p><p>The reader is on this line now.</p>";

  function anchorIn(html: string, text: string) {
    const root = article(html);
    const node = [...root.querySelectorAll("p")].find((p) =>
      p.textContent?.includes(text),
    )!.firstChild!;
    const range = textRangeFrom(root, node, node.textContent!.indexOf(text), text.length)!;
    const anchor = { selectors: rangeToSelectors(range, root), top: 0 };
    root.remove();
    return anchor;
  }

  it("finds the text after an edit above it", () => {
    const anchor = anchorIn(before, "reader is on this line");
    const root = article(`<p>Added first.</p>${before}`);

    expect(findReadingAnchor(root, anchor)?.toString()).toBe("reader is on this line");
  });

  it("does not settle for a fuzzy match", () => {
    const anchor = anchorIn(before, "reader is on this line");
    const root = article("<p>One line here.</p><p>The reader is on that line now.</p>");

    expect(findReadingAnchor(root, anchor)).toBeNull();
  });
});
