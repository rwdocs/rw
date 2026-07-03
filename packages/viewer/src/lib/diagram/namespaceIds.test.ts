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

  it("rewrites #id CSS selectors in <style> so an id-scoped stylesheet still styles the clone", () => {
    // Shape of a Kroki Mermaid SVG: root id + every rule prefixed with it.
    const el = svg(
      `<style>#container .node rect{fill:#ECECFF;}#container .marker{fill:#333;}#container p{color:#ffffde;}</style>` +
        `<g class="node"><rect></rect></g>`,
    );
    el.setAttribute("id", "container");

    namespaceIds(el);

    const newId = el.getAttribute("id")!;
    expect(newId).toMatch(/^dzm\d+-container$/);
    expect(el.querySelector("style")!.textContent).toBe(
      `#${newId} .node rect{fill:#ECECFF;}#${newId} .marker{fill:#333;}#${newId} p{color:#ffffde;}`,
    );
  });

  it("leaves unmapped #tokens in <style> alone (hex colors, external ids, longer idents)", () => {
    const el = svg(
      `<style>#container rect{fill:#eef;}#external{fill:#aaaa33;}#containerfoo{stroke:#eef;}</style>` +
        `<rect id="container"></rect>`,
    );

    namespaceIds(el);

    // The mapped id's selector is rewritten, but #external, #containerfoo (a
    // longer ident), and letter-led hex colors are not mapped ids — untouched.
    const newId = el.querySelector("rect")!.id;
    expect(el.querySelector("style")!.textContent).toBe(
      `#${newId} rect{fill:#eef;}#external{fill:#aaaa33;}#containerfoo{stroke:#eef;}`,
    );
  });

  it("rewrites #id CSS selectors for non-ASCII ids in lockstep with the id attribute", () => {
    const el = svg(`<style>#узел1 rect{fill:#eef;}</style><g id="узел1"><rect></rect></g>`);

    namespaceIds(el);

    const newId = el.querySelector("g")!.getAttribute("id")!;
    expect(newId).toMatch(/^dzm\d+-узел1$/);
    expect(el.querySelector("style")!.textContent).toBe(`#${newId} rect{fill:#eef;}`);
  });

  it("rewrites id selectors and non-ident url(#id) fragments in the same <style>", () => {
    // Leading-digit ids (e.g. "123-grad") can't be a CSS selector token, only
    // a url() fragment — exercises that URL_REF still fires inside <style>
    // even though CSS_ID_TOKEN can't match this id.
    const el = svg(
      `<style>#container rect{fill:url(#123-grad);}.b{stroke:url('#123-grad');}</style>` +
        `<linearGradient id="123-grad"></linearGradient><rect id="container"></rect>`,
    );

    namespaceIds(el);

    const containerId = el.querySelector("rect")!.id;
    const gradId = el.querySelector("linearGradient")!.id;
    expect(gradId).toMatch(/^dzm\d+-123-grad$/);
    expect(el.querySelector("style")!.textContent).toBe(
      `#${containerId} rect{fill:url(#${gradId});}.b{stroke:url(#${gradId});}`,
    );
  });

  it("rewrites aria-labelledby / aria-describedby idref lists", () => {
    const el = svg(`<title id="chart-title">T</title><desc id="chart-desc">D</desc>`);
    el.setAttribute("aria-labelledby", "chart-title");
    el.setAttribute("aria-describedby", "chart-desc chart-title missing-id");

    namespaceIds(el);

    const titleId = el.querySelector("title")!.id;
    const descId = el.querySelector("desc")!.id;
    expect(titleId).toMatch(/^dzm\d+-chart-title$/);
    expect(el.getAttribute("aria-labelledby")).toBe(titleId);
    // Every mapped token rewritten (not just the first), unknown token
    // preserved, separators preserved.
    expect(el.getAttribute("aria-describedby")).toBe(`${descId} ${titleId} missing-id`);
  });
});
