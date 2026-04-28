// jsdom does not implement ResizeObserver, and even if it did we need
// programmatic control over the callback to simulate a resize. A minimal mock
// is enough: store the callback, track observed elements, expose a `trigger`
// helper for tests.
export class MockResizeObserver {
  static instances: MockResizeObserver[] = [];
  callback: ResizeObserverCallback;
  observed: Element[] = [];
  disconnected = false;

  constructor(cb: ResizeObserverCallback) {
    this.callback = cb;
    MockResizeObserver.instances.push(this);
  }

  observe(el: Element) {
    this.observed.push(el);
  }

  unobserve() {}

  disconnect() {
    this.disconnected = true;
    this.observed = [];
  }

  trigger() {
    this.callback(
      this.observed.map((target) => ({ target }) as ResizeObserverEntry),
      this as unknown as ResizeObserver,
    );
  }
}

// Not redefining getBoundingClientRect on the prototype keeps other tests
// (and any unrelated rects read from the document root) untouched.
export function makeAnchor(rect: Partial<DOMRect>): HTMLElement {
  const el = document.createElement("div");
  const get = () => ({
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
  });
  el.getBoundingClientRect = () => get() as DOMRect;
  return el;
}
