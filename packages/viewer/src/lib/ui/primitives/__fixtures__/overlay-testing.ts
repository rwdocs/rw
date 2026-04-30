// Shared test helpers for anchored-overlay primitives (Popover, Menu, ...).
// jsdom does not ship a ResizeObserver, which Popover's anchored mode relies
// on via useAnchorOffset — re-export the richer mock from the hooks fixture.
// Primitives tests don't need `instances`/`trigger()`, but a single shared
// implementation keeps the global stub consistent across suites.
import { fakeRect } from "../../hooks/__fixtures__/resize-observer-mock";

export { MockResizeObserver, fakeRect } from "../../hooks/__fixtures__/resize-observer-mock";

export function mockRect(el: HTMLElement, rect: Partial<DOMRect>): void {
  el.getBoundingClientRect = () => fakeRect(rect);
}

export function createAnchor(): HTMLButtonElement {
  const el = document.createElement("button");
  el.type = "button";
  mockRect(el, { top: 0, left: 0, width: 50, height: 20 });
  document.body.appendChild(el);
  return el;
}
