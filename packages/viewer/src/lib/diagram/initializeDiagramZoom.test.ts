import { describe, it, expect, vi } from "vitest";
import { initializeDiagramZoom, EXPAND_BUTTON_CLASS } from "./initializeDiagramZoom";

function container(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

const DIAGRAM = `<figure class="diagram"><svg viewBox="0 0 10 10"></svg></figure>`;

describe("initializeDiagramZoom", () => {
  it("injects one expand button per diagram figure", () => {
    const el = container(DIAGRAM + DIAGRAM);
    initializeDiagramZoom(el, () => {});
    expect(el.querySelectorAll(`.${EXPAND_BUTTON_CLASS}`)).toHaveLength(2);
  });

  it("gives the button an accessible label and button type", () => {
    const el = container(DIAGRAM);
    initializeDiagramZoom(el, () => {});
    const btn = el.querySelector<HTMLButtonElement>(`.${EXPAND_BUTTON_CLASS}`)!;
    expect(btn.getAttribute("aria-label")).toBe("Expand diagram");
    expect(btn.type).toBe("button");
  });

  it("skips error figures", () => {
    const el = container(`<figure class="diagram diagram-error"><pre>boom</pre></figure>`);
    initializeDiagramZoom(el, () => {});
    expect(el.querySelectorAll(`.${EXPAND_BUTTON_CLASS}`)).toHaveLength(0);
  });

  it("calls onOpen with the figure when the button is clicked", () => {
    const el = container(DIAGRAM);
    const onOpen = vi.fn();
    initializeDiagramZoom(el, onOpen);
    el.querySelector<HTMLButtonElement>(`.${EXPAND_BUTTON_CLASS}`)!.click();
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen.mock.calls[0][0]).toBe(el.querySelector("figure.diagram"));
  });

  it("does not double-inject when run twice on the same DOM", () => {
    const el = container(DIAGRAM);
    initializeDiagramZoom(el, () => {});
    initializeDiagramZoom(el, () => {});
    expect(el.querySelectorAll(`.${EXPAND_BUTTON_CLASS}`)).toHaveLength(1);
  });

  it("cleanup removes the injected buttons", () => {
    const el = container(DIAGRAM);
    const cleanup = initializeDiagramZoom(el, () => {});
    cleanup();
    expect(el.querySelectorAll(`.${EXPAND_BUTTON_CLASS}`)).toHaveLength(0);
  });

  it("cleanup detaches the click handler", () => {
    const el = container(DIAGRAM);
    const onOpen = vi.fn();
    const cleanup = initializeDiagramZoom(el, onOpen);
    const btn = el.querySelector<HTMLButtonElement>(`.${EXPAND_BUTTON_CLASS}`)!;
    cleanup();
    btn.click(); // detached from DOM, but ensure no handler fires either
    expect(onOpen).not.toHaveBeenCalled();
  });
});
