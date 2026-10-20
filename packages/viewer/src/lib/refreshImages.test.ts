import { describe, it, expect } from "vitest";
import { refreshImages } from "./refreshImages";

function article(html: string): HTMLElement {
  const root = document.createElement("article");
  root.innerHTML = html;
  return root;
}

const srcs = (root: HTMLElement) =>
  [...root.querySelectorAll("img")].map((img) => img.getAttribute("src"));

describe("refreshImages", () => {
  it("leaves images as rendered at version 0", () => {
    const root = article('<img src="/_api/files/docs/a.png">');

    refreshImages(root, 0);

    expect(srcs(root)).toEqual(["/_api/files/docs/a.png"]);
  });

  it("versions same-origin images, keeping their query and fragment", () => {
    const root = article(
      '<img src="/_api/files/docs/a%20b.png"><img src="/_api/files/x.svg?v=2#frag">',
    );

    refreshImages(root, 3);

    expect(srcs(root)).toEqual([
      "/_api/files/docs/a%20b.png?rw-files=3",
      "/_api/files/x.svg?v=2&rw-files=3#frag",
    ]);
  });

  it("replaces an earlier version instead of adding another", () => {
    const root = article('<img src="/_api/files/a.png">');

    refreshImages(root, 1);
    refreshImages(root, 2);

    expect(srcs(root)).toEqual(["/_api/files/a.png?rw-files=2"]);
  });

  it("leaves an empty, query-only or fragment-only src alone", () => {
    const root = article('<img src=""><img src="?x"><img src="#y"><img>');

    refreshImages(root, 1);

    expect(srcs(root)).toEqual(["", "?x", "#y", null]);
  });

  it("leaves other origins and data URLs alone", () => {
    const html = '<img src="https://example.com/a.png"><img src="data:image/png;base64,AAAA">';
    const root = article(html);

    refreshImages(root, 1);

    expect(srcs(root)).toEqual(["https://example.com/a.png", "data:image/png;base64,AAAA"]);
  });
});
