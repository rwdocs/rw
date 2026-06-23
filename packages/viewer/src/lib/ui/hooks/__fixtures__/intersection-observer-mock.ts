// jsdom does not implement IntersectionObserver, and `observeMove` needs
// programmatic control over the callback to simulate the element moving. A
// minimal mock: store the callback + options, track observed elements, expose
// a `trigger(ratio)` helper. Mirrors the ResizeObserver mock in this folder.
export class MockIntersectionObserver {
  static instances: MockIntersectionObserver[] = [];
  callback: IntersectionObserverCallback;
  options?: IntersectionObserverInit;
  observed: Element[] = [];
  disconnected = false;

  constructor(cb: IntersectionObserverCallback, options?: IntersectionObserverInit) {
    this.callback = cb;
    this.options = options;
    MockIntersectionObserver.instances.push(this);
  }

  observe(el: Element) {
    this.observed.push(el);
  }

  unobserve() {}

  disconnect() {
    this.disconnected = true;
    this.observed = [];
  }

  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }

  /** Simulate an intersection change at the given ratio (1 = unmoved). */
  trigger(ratio: number) {
    this.callback(
      this.observed.map(
        (target) => ({ target, intersectionRatio: ratio }) as IntersectionObserverEntry,
      ),
      this as unknown as IntersectionObserver,
    );
  }

  /** The most recently constructed (currently-armed) observer. */
  static get latest(): MockIntersectionObserver | undefined {
    return MockIntersectionObserver.instances[MockIntersectionObserver.instances.length - 1];
  }
}
