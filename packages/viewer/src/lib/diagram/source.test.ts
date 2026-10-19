import { beforeAll, describe, expect, it } from "vitest";
import { registerRwDiagram } from "./rwDiagramElement";
import { diagramShadowRoots, diagramSource } from "./source";

beforeAll(() => registerRwDiagram());

function figureFrom(html: string): HTMLElement {
  const host = document.createElement("div");
  document.body.appendChild(host);
  host.innerHTML = html;
  return host.firstElementChild as HTMLElement;
}

describe("diagramSource", () => {
  it("finds the svg inside a wrapped figure", () => {
    const fig = figureFrom(
      `<figure class="diagram"><rw-diagram><svg id="s"></svg></rw-diagram></figure>`,
    );
    expect(diagramSource(fig)?.id).toBe("s");
  });

  it("finds a direct-child svg in an un-wrapped figure", () => {
    const fig = figureFrom(`<figure class="diagram"><svg id="raw"></svg></figure>`);
    expect(diagramSource(fig)?.id).toBe("raw");
  });

  it("finds a PNG img in either shape", () => {
    const wrapped = figureFrom(
      `<figure class="diagram"><rw-diagram><img src="a.png"></rw-diagram></figure>`,
    );
    const raw = figureFrom(`<figure class="diagram"><img src="b.png"></figure>`);
    expect((diagramSource(wrapped) as HTMLImageElement).getAttribute("src")).toBe("a.png");
    expect((diagramSource(raw) as HTMLImageElement).getAttribute("src")).toBe("b.png");
  });

  // What markdown makes of hand-authored content: CommonMark wraps it in `<p>`
  // when blank lines surround it, and `[![x](a.png)](url)` adds a link.
  it.each([
    ["paragraph svg", `<p><svg id="d"></svg></p>`],
    ["paragraph img", `<p><img id="d" src="c.png"></p>`],
    ["linked img", `<p><a href="/x"><img id="d" src="c.png"></a></p>`],
    ["linked svg", `<p><a href="/x"><svg id="d"></svg></a></p>`],
    ["picture img", `<picture><source srcset="c.webp"><img id="d" src="c.png"></picture>`],
    ["div svg", `<div><svg id="d"></svg></div>`],
  ])("finds a %s in an un-wrapped figure", (_, inner) => {
    const fig = figureFrom(`<figure class="diagram">${inner}</figure>`);
    expect(diagramSource(fig)?.id).toBe("d");
  });

  it("returns the outer svg of a nested one", () => {
    const fig = figureFrom(
      `<figure class="diagram"><svg id="outer"><svg id="inner"></svg></svg></figure>`,
    );
    expect(diagramSource(fig)?.id).toBe("outer");
  });

  it("skips the expand-button icon even when it comes first", () => {
    const fig = figureFrom(
      `<figure class="diagram"><button class="diagram-expand-btn"><svg id="icon"></svg></button>` +
        `<p><svg id="d"></svg></p></figure>`,
    );
    expect(diagramSource(fig)?.id).toBe("d");
  });

  it("ignores the injected expand-button icon", () => {
    // A figure whose diagram failed to produce any svg still carries the
    // button's own icon <svg>; an unscoped lookup would return that.
    const fig = figureFrom(
      `<figure class="diagram"><button class="diagram-expand-btn"><svg id="icon"></svg></button></figure>`,
    );
    expect(diagramSource(fig)).toBeNull();
  });

  it("returns null for a figure with no diagram", () => {
    expect(diagramSource(figureFrom(`<figure class="diagram"></figure>`))).toBeNull();
  });
});

describe("diagramShadowRoots", () => {
  it("collects every wrapped diagram's root", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.innerHTML =
      `<figure class="diagram"><rw-diagram><svg id="a"></svg></rw-diagram></figure>` +
      `<figure class="diagram"><svg id="raw"></svg></figure>` +
      `<figure class="diagram"><rw-diagram><svg id="b"></svg></rw-diagram></figure>`;

    const roots = diagramShadowRoots(host);
    expect(roots).toHaveLength(2);
    expect(roots[0].querySelector("svg")?.id).toBe("a");
    expect(roots[1].querySelector("svg")?.id).toBe("b");
  });

  it("returns an empty list when there are no diagrams", () => {
    const host = document.createElement("div");
    host.innerHTML = `<p>no diagrams</p>`;
    expect(diagramShadowRoots(host)).toEqual([]);
  });
});
