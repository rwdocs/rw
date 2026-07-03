import { describe, it, expect, afterEach } from "vitest";
import { isInDiagram, rangeTouchesDiagram, diagramExclusionFilter } from "./diagram";

afterEach(() => {
  document.body.innerHTML = "";
});

function article(html: string): HTMLElement {
  const el = document.createElement("article");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

/** Range over the first text node whose data contains `needle`. */
function rangeOver(root: Element, needle: string): Range {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {
    const t = walker.currentNode as Text;
    const i = t.data.indexOf(needle);
    if (i !== -1) {
      const r = document.createRange();
      r.setStart(t, i);
      r.setEnd(t, i + needle.length);
      return r;
    }
  }
  throw new Error(`"${needle}" not found`);
}

describe("diagram predicate", () => {
  const HTML = `<p>before text</p><figure class="diagram"><svg><text>Billing</text></svg></figure><p>after text</p>`;

  it("isInDiagram: true inside figure.diagram, false in prose", () => {
    const a = article(HTML);
    const label = a.querySelector("text")!.firstChild!;
    const prose = a.querySelector("p")!.firstChild!;
    expect(isInDiagram(label)).toBe(true);
    expect(isInDiagram(prose)).toBe(false);
    expect(isInDiagram(a.querySelector("figure")!)).toBe(true);
  });

  it("rangeTouchesDiagram: true for a selection inside a diagram", () => {
    const a = article(HTML);
    expect(rangeTouchesDiagram(rangeOver(a, "Billing"), a)).toBe(true);
  });

  it("rangeTouchesDiagram: true for a selection spanning a diagram (prose→diagram→prose)", () => {
    const a = article(HTML);
    const before = a.querySelectorAll("p")[0].firstChild as Text;
    const after = a.querySelectorAll("p")[1].firstChild as Text;
    const r = document.createRange();
    r.setStart(before, 0);
    r.setEnd(after, 3);
    expect(rangeTouchesDiagram(r, a)).toBe(true);
  });

  it("rangeTouchesDiagram: false for a pure-prose selection", () => {
    const a = article(HTML);
    expect(rangeTouchesDiagram(rangeOver(a, "before"), a)).toBe(false);
  });

  it("diagramExclusionFilter rejects diagram text nodes, accepts prose", () => {
    const a = article(HTML);
    const label = a.querySelector("text")!.firstChild!;
    const prose = a.querySelector("p")!.firstChild!;
    // NodeFilter is typed as a function-or-object union; narrow to the object form.
    const accept = (n: Node) =>
      (diagramExclusionFilter as { acceptNode(node: Node): number }).acceptNode(n);
    expect(accept(label)).toBe(NodeFilter.FILTER_REJECT);
    expect(accept(prose)).toBe(NodeFilter.FILTER_ACCEPT);
  });
});
