// Shared test helpers for anchored-overlay primitives (Popover, Menu, ...).
// jsdom does not ship a ResizeObserver, which Popover's anchored mode relies
// on via useAnchorOffset — stub it with a no-op; the initial rect measurement
// happens synchronously during the hook's first effect.
export class MockResizeObserver {
  callback: ResizeObserverCallback;
  constructor(cb: ResizeObserverCallback) {
    this.callback = cb;
  }
  observe() {}
  unobserve() {}
  disconnect() {}
}

export function mockRect(el: HTMLElement, rect: Partial<DOMRect>): void {
  el.getBoundingClientRect = () =>
    ({
      top: 0,
      left: 0,
      width: 0,
      height: 0,
      right: 0,
      bottom: 0,
      x: 0,
      y: 0,
      toJSON: () => ({}),
      ...rect,
    }) as DOMRect;
}

export function createAnchor(): HTMLButtonElement {
  const el = document.createElement("button");
  el.type = "button";
  mockRect(el, { top: 0, left: 0, width: 50, height: 20 });
  document.body.appendChild(el);
  return el;
}
