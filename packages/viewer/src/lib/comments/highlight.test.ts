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
