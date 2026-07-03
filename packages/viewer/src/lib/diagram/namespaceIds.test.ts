import { describe, it, expect } from "vitest";
import { namespaceIds } from "./namespaceIds";

const SVG_NS = "http://www.w3.org/2000/svg";
const XLINK_NS = "http://www.w3.org/1999/xlink";

function svg(inner: string): SVGSVGElement {
  const el = document.createElementNS(SVG_NS, "svg");
  el.innerHTML = inner;
  return el as SVGSVGElement;
}

describe("namespaceIds", () => {
  it("rewrites ids and url(#id) references so the clone points at its own defs", () => {
    const el = svg(`
      <defs>
        <marker id="arrow"></marker>
        <linearGradient id="grad"></linearGradient>
        <clipPath id="clip"></clipPath>
      </defs>
      <path marker-end="url(#arrow)" fill="url(#grad)" clip-path="url(#clip)"></path>
    `);

    namespaceIds(el);

    const marker = el.querySelector("marker")!;
    const path = el.querySelector("path")!;
    // ids were renamed away from the originals...
    expect(marker.id).not.toBe("arrow");
    expect(marker.id).toMatch(/^dzm\d+-arrow$/);
    // ...and every reference now points at the renamed element inside the clone.
    expect(path.getAttribute("marker-end")).toBe(`url(#${marker.id})`);
    expect(path.getAttribute("fill")).toBe(`url(#${el.querySelector("linearGradient")!.id})`);
    expect(path.getAttribute("clip-path")).toBe(`url(#${el.querySelector("clipPath")!.id})`);
  });

  it("rewrites url(#id) inside inline style and <style> blocks", () => {
    const el = svg(`
      <style>.n { fill: url(#grad); }</style>
      <linearGradient id="grad"></linearGradient>
      <rect style="fill:url(#grad)"></rect>
    `);

    namespaceIds(el);
    const newId = el.querySelector("linearGradient")!.id;
    expect(el.querySelector("rect")!.getAttribute("style")).toBe(`fill:url(#${newId})`);
    expect(el.querySelector("style")!.textContent).toContain(`url(#${newId})`);
  });

  it("rewrites href and xlink:href fragment references", () => {
    const el = svg(`<linearGradient id="base"></linearGradient>`);
    // SVG2 href
    const grad = document.createElementNS(SVG_NS, "radialGradient");
    grad.setAttribute("id", "g2");
    grad.setAttribute("href", "#base");
    el.appendChild(grad);
    // legacy xlink:href
    const use = document.createElementNS(SVG_NS, "use");
    use.setAttributeNS(XLINK_NS, "xlink:href", "#base");
    el.appendChild(use);

    namespaceIds(el);
    const baseId = el.querySelector("linearGradient")!.id;
    expect(baseId).toMatch(/^dzm\d+-base$/);
    expect(grad.getAttribute("href")).toBe(`#${baseId}`);
    expect(use.getAttributeNS(XLINK_NS, "href")).toBe(`#${baseId}`);
  });

  it("leaves references to ids not defined inside the svg untouched", () => {
    const el = svg(`<path fill="url(#external)" marker-end="url(#arrow)"></path>
      <marker id="arrow"></marker>`);
    namespaceIds(el);
    const path = el.querySelector("path")!;
    expect(path.getAttribute("fill")).toBe("url(#external)"); // unknown id preserved
    expect(path.getAttribute("marker-end")).toBe(`url(#${el.querySelector("marker")!.id})`);
  });

  it("uses a fresh prefix per call so two clones never collide", () => {
    const a = svg(`<marker id="arrow"></marker>`);
    const b = svg(`<marker id="arrow"></marker>`);
    namespaceIds(a);
    namespaceIds(b);
    expect(a.querySelector("marker")!.id).not.toBe(b.querySelector("marker")!.id);
  });

  it("is a no-op for an svg with no ids", () => {
    const el = svg(`<path d="M0 0h10"></path>`);
    const before = el.innerHTML;
    namespaceIds(el);
    expect(el.innerHTML).toBe(before);
  });
});
