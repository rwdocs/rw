import { describe, it, expect, afterEach } from "vitest";
import { unwrapAll, wrapRange } from "./highlight";

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
    const container = createContainer("<p>Hello <rw-annotation data-comment-id=\"a\">world</rw-annotation>!</p>");
    unwrapAll(container);
    expect(container.innerHTML).toBe("<p>Hello world!</p>");
    expect(container.querySelectorAll("rw-annotation")).toHaveLength(0);
  });

  it("removes nested rw-annotation elements", () => {
    const container = createContainer(
      "<p><rw-annotation data-comment-id=\"a\">foo <rw-annotation data-comment-id=\"b\">bar</rw-annotation> baz</rw-annotation></p>",
    );
    unwrapAll(container);
    expect(container.innerHTML).toBe("<p>foo bar baz</p>");
  });

  it("merges adjacent text nodes via normalize()", () => {
    const container = createContainer("<p>Hello <rw-annotation data-comment-id=\"a\">world</rw-annotation>!</p>");
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
