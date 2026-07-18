import { beforeAll, describe, expect, it } from "vitest";
import { registerRwDiagram } from "./rwDiagramElement";

beforeAll(() => registerRwDiagram());

describe("rw-diagram", () => {
  it("moves its children into an open shadow root", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.innerHTML = `<rw-diagram><svg id="clip1"></svg></rw-diagram>`;

    const el = host.querySelector("rw-diagram")!;
    expect(el.shadowRoot).not.toBeNull();
    expect(el.shadowRoot!.querySelector("svg")?.id).toBe("clip1");
    // The SVG must no longer be reachable from the light tree.
    expect(el.querySelector("svg")).toBeNull();
  });

  it("isolates ids between two diagrams", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.innerHTML =
      `<rw-diagram><svg id="a"><clipPath id="clip1"></clipPath></svg></rw-diagram>` +
      `<rw-diagram><svg id="b"><clipPath id="clip1"></clipPath></svg></rw-diagram>`;

    const [first, second] = [...host.querySelectorAll("rw-diagram")];
    const c1 = first.shadowRoot!.getElementById("clip1");
    const c2 = second.shadowRoot!.getElementById("clip1");
    expect(c1).not.toBeNull();
    expect(c2).not.toBeNull();
    // Each root resolves its OWN clip1 — the whole point of the change.
    expect(c1).not.toBe(c2);
    // And neither leaks into the document scope.
    expect(document.getElementById("clip1")).toBeNull();
  });

  it("is safe to register twice", () => {
    expect(() => {
      registerRwDiagram();
      registerRwDiagram();
    }).not.toThrow();
  });

  it("does not re-attach a shadow root when re-connected", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.innerHTML = `<rw-diagram><svg id="x"></svg></rw-diagram>`;
    const el = host.querySelector("rw-diagram")!;
    const root = el.shadowRoot;

    el.remove();
    host.appendChild(el);

    expect(el.shadowRoot).toBe(root);
    expect(el.shadowRoot!.querySelector("svg")?.id).toBe("x");
  });
});
