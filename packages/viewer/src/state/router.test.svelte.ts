import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { extractDocPath, Router } from "./router.svelte";

describe("extractDocPath", () => {
  it("strips leading slash from path", () => {
    expect(extractDocPath("/docs/page")).toBe("docs/page");
  });

  it("handles root path", () => {
    expect(extractDocPath("/")).toBe("");
  });

  it("handles path without leading slash", () => {
    expect(extractDocPath("docs/page")).toBe("docs/page");
  });

  it("handles nested paths", () => {
    expect(extractDocPath("/domain/subdomain/page")).toBe("domain/subdomain/page");
  });
});

describe("goto", () => {
  beforeEach(() => {
    Object.defineProperty(window, "location", {
      value: { origin: "http://localhost:8001", pathname: "/", hash: "" },
      writable: true,
      configurable: true,
    });
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("pushes new path to history", () => {
    const router = new Router();
    router.goto("/new-path");

    expect(window.history.pushState).toHaveBeenCalledWith({}, "", "/new-path");
  });

  it("updates path store", () => {
    const router = new Router();
    router.goto("/new-path");

    expect(router.path).toBe("/new-path");
  });

  it("updates hash store when hash is present", () => {
    const router = new Router();
    router.goto("/new-path#section");

    expect(router.path).toBe("/new-path");
    expect(router.hash).toBe("section");
  });

  it("clears hash store when hash is not present", () => {
    const router = new Router();
    router.goto("/new-path");

    expect(router.hash).toBe("");
  });

  // Scroll-to-top on navigation is handled by Layout component
  // which scrolls the actual content container element

  it("notifies path change listeners on goto", () => {
    const router = new Router();
    const listener = vi.fn();
    router.onPathChange(listener);

    router.goto("/new-path");

    expect(listener).toHaveBeenCalledOnce();
  });

  it("does not notify after unsubscribe", () => {
    const router = new Router();
    const listener = vi.fn();
    const unsub = router.onPathChange(listener);

    unsub();
    router.goto("/new-path");

    expect(listener).not.toHaveBeenCalled();
  });
});

describe("initRouter", () => {
  let popstateHandler: ((e: PopStateEvent) => void) | null = null;
  let clickHandler: ((e: MouseEvent) => void) | null = null;
  let cleanup: (() => void) | null = null;
  let router: Router;

  beforeEach(() => {
    Object.defineProperty(window, "location", {
      value: { origin: "http://localhost:8001", pathname: "/", hash: "" },
      writable: true,
      configurable: true,
    });
    // Capture event handlers
    vi.spyOn(window, "addEventListener").mockImplementation((event, handler) => {
      if (event === "popstate") popstateHandler = handler as (e: PopStateEvent) => void;
    });
    vi.spyOn(document, "addEventListener").mockImplementation((event, handler) => {
      if (event === "click") clickHandler = handler as (e: MouseEvent) => void;
    });
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});

    router = new Router();
    cleanup = router.initRouter();
  });

  afterEach(() => {
    cleanup?.();
    vi.restoreAllMocks();
    popstateHandler = null;
    clickHandler = null;
    cleanup = null;
  });

  it("registers popstate listener", () => {
    expect(window.addEventListener).toHaveBeenCalledWith("popstate", expect.any(Function));
  });

  it("registers click listener", () => {
    expect(document.addEventListener).toHaveBeenCalledWith("click", expect.any(Function));
  });

  describe("popstate handler", () => {
    it("updates path store on back/forward navigation", () => {
      Object.defineProperty(window, "location", {
        value: { pathname: "/back-path", hash: "" },
        writable: true,
        configurable: true,
      });

      popstateHandler!({} as PopStateEvent);

      expect(router.path).toBe("/back-path");
      expect(router.hash).toBe("");
    });

    it("updates hash store when navigating to URL with hash", () => {
      Object.defineProperty(window, "location", {
        value: { pathname: "/back-path", hash: "#section" },
        writable: true,
        configurable: true,
      });

      popstateHandler!({} as PopStateEvent);

      expect(router.path).toBe("/back-path");
      expect(router.hash).toBe("section");
    });

    it("notifies path change listeners on popstate", () => {
      const listener = vi.fn();
      router.onPathChange(listener);

      Object.defineProperty(window, "location", {
        value: { pathname: "/back-path", hash: "" },
        writable: true,
        configurable: true,
      });

      popstateHandler!({} as PopStateEvent);

      expect(listener).toHaveBeenCalledOnce();
    });
  });

  describe("click handler", () => {
    function createClickEvent(
      options: { metaKey?: boolean; ctrlKey?: boolean; shiftKey?: boolean; altKey?: boolean } = {},
    ): MouseEvent {
      const event = new MouseEvent("click", {
        bubbles: true,
        cancelable: true,
        ...options,
      });
      vi.spyOn(event, "preventDefault");
      return event;
    }

    function createAnchor(
      href: string | null,
      attributes: Record<string, string> = {},
    ): HTMLAnchorElement {
      const anchor = document.createElement("a");
      if (href !== null) anchor.setAttribute("href", href);
      for (const [key, value] of Object.entries(attributes)) {
        anchor.setAttribute(key, value);
      }
      return anchor;
    }

    it("navigates on internal link click", () => {
      const anchor = createAnchor("/internal-page");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/internal-page");
    });

    it("ignores clicks not on anchors", () => {
      const div = document.createElement("div");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: div });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores anchors without href", () => {
      const anchor = createAnchor(null);
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores clicks with meta key", () => {
      const anchor = createAnchor("/page");
      const event = createClickEvent({ metaKey: true });
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores clicks with ctrl key", () => {
      const anchor = createAnchor("/page");
      const event = createClickEvent({ ctrlKey: true });
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores clicks with shift key", () => {
      const anchor = createAnchor("/page");
      const event = createClickEvent({ shiftKey: true });
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores clicks with alt key", () => {
      const anchor = createAnchor("/page");
      const event = createClickEvent({ altKey: true });
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores external http links", () => {
      const anchor = createAnchor("https://example.com");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores protocol-relative links", () => {
      const anchor = createAnchor("//example.com/page");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores mailto links", () => {
      const anchor = createAnchor("mailto:test@example.com");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores tel links", () => {
      const anchor = createAnchor("tel:+1234567890");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores hash links", () => {
      const anchor = createAnchor("#section");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores links with target attribute", () => {
      const anchor = createAnchor("/page", { target: "_blank" });
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("ignores links with download attribute", () => {
      const anchor = createAnchor("/file.pdf", { download: "" });
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: anchor });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it("handles clicks on nested elements inside anchor", () => {
      const anchor = createAnchor("/nested-page");
      const span = document.createElement("span");
      anchor.appendChild(span);
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: span });

      clickHandler!(event);

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/nested-page");
    });
  });
});

describe("embedded mode", () => {
  beforeEach(() => {
    Object.defineProperty(window, "location", {
      value: { origin: "http://localhost:8001", pathname: "/", hash: "" },
      writable: true,
      configurable: true,
    });
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("uses initialPath as starting path in embedded mode", () => {
    const router = new Router({ embedded: true, initialPath: "/guide" });

    expect(router.path).toBe("/guide");
  });

  it("parses hash from initialPath in embedded mode", () => {
    const router = new Router({ embedded: true, initialPath: "/guide#section" });

    expect(router.path).toBe("/guide");
    expect(router.hash).toBe("section");
  });

  it("initializes hash to empty when initialPath has no hash in embedded mode", () => {
    const router = new Router({ embedded: true, initialPath: "/guide" });

    expect(router.hash).toBe("");
  });

  it("ignores initialPath in normal mode", () => {
    const router = new Router({ embedded: false, initialPath: "/guide" });

    expect(router.path).toBe("/");
  });

  it("defaults to / when no initialPath in embedded mode", () => {
    Object.defineProperty(window, "location", {
      value: { origin: "http://localhost:8001", pathname: "/backstage/catalog", hash: "" },
      writable: true,
      configurable: true,
    });

    const router = new Router({ embedded: true });

    // Should default to "/" not the host app's pathname
    expect(router.path).toBe("/");
  });

  it("goto does not call pushState in embedded mode", () => {
    const router = new Router({ embedded: true });
    router.goto("/guide");

    expect(window.history.pushState).not.toHaveBeenCalled();
    expect(router.path).toBe("/guide");
  });

  it("goto calls onNavigate callback in embedded mode", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.goto("/guide");

    expect(onNavigate).toHaveBeenCalledWith("/guide");
    expect(window.history.pushState).not.toHaveBeenCalled();
  });

  it("goto does not call onNavigate in normal mode", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: false, onNavigate });
    router.goto("/guide");

    expect(onNavigate).not.toHaveBeenCalled();
    expect(window.history.pushState).toHaveBeenCalled();
  });

  it("goto calls pushState in normal mode", () => {
    const router = new Router({ embedded: false });
    router.goto("/guide");

    expect(window.history.pushState).toHaveBeenCalled();
  });

  it("initRouter skips popstate listener in embedded mode", () => {
    const addEventSpy = vi.spyOn(window, "addEventListener");
    const router = new Router({ embedded: true });
    const cleanup = router.initRouter();

    const popstateCall = addEventSpy.mock.calls.find(([event]) => event === "popstate");
    expect(popstateCall).toBeUndefined();

    cleanup();
    addEventSpy.mockRestore();
  });

  it("initRouter registers popstate listener in normal mode", () => {
    const addEventSpy = vi.spyOn(window, "addEventListener");
    const router = new Router({ embedded: false });
    const cleanup = router.initRouter();

    const popstateCall = addEventSpy.mock.calls.find(([event]) => event === "popstate");
    expect(popstateCall).toBeDefined();

    cleanup();
    addEventSpy.mockRestore();
  });

  it("initRouter scopes click handler to root element in embedded mode", () => {
    const rootElement = document.createElement("div");
    const rootAddEventSpy = vi.spyOn(rootElement, "addEventListener");
    const docAddEventSpy = vi.spyOn(document, "addEventListener");

    const router = new Router({ embedded: true });
    const cleanup = router.initRouter(rootElement);

    const rootClickCall = rootAddEventSpy.mock.calls.find(([event]) => event === "click");
    expect(rootClickCall).toBeDefined();

    const docClickCall = docAddEventSpy.mock.calls.find(([event]) => event === "click");
    expect(docClickCall).toBeUndefined();

    cleanup();
    rootAddEventSpy.mockRestore();
    docAddEventSpy.mockRestore();
  });

  it("initRouter attaches click handler to document in normal mode", () => {
    const rootElement = document.createElement("div");
    const rootAddEventSpy = vi.spyOn(rootElement, "addEventListener");
    const docAddEventSpy = vi.spyOn(document, "addEventListener");

    const router = new Router({ embedded: false });
    const cleanup = router.initRouter(rootElement);

    const rootClickCall = rootAddEventSpy.mock.calls.find(([event]) => event === "click");
    expect(rootClickCall).toBeUndefined();

    const docClickCall = docAddEventSpy.mock.calls.find(([event]) => event === "click");
    expect(docClickCall).toBeDefined();

    cleanup();
    rootAddEventSpy.mockRestore();
    docAddEventSpy.mockRestore();
  });

  it("initRouter falls back to document when no root element in embedded mode", () => {
    const docAddEventSpy = vi.spyOn(document, "addEventListener");

    const router = new Router({ embedded: true });
    const cleanup = router.initRouter();

    const docClickCall = docAddEventSpy.mock.calls.find(([event]) => event === "click");
    expect(docClickCall).toBeDefined();

    cleanup();
    docAddEventSpy.mockRestore();
  });
});

describe("basePath", () => {
  beforeEach(() => {
    Object.defineProperty(window, "location", {
      value: { origin: "http://localhost:3000", pathname: "/", hash: "" },
      writable: true,
      configurable: true,
    });
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("prefixPath returns path unchanged when no basePath", () => {
    const router = new Router({ embedded: true });
    expect(router.prefixPath("/docs/guide")).toBe("/docs/guide");
  });

  it("prefixPath prepends basePath to path", () => {
    const router = new Router({ embedded: true, basePath: "/rw-docs" });
    expect(router.prefixPath("/docs/guide")).toBe("/rw-docs/docs/guide");
  });

  it("prefixPath handles root path", () => {
    const router = new Router({ embedded: true, basePath: "/rw-docs" });
    expect(router.prefixPath("/")).toBe("/rw-docs/");
  });

  it("click handler strips basePath from href before navigating", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, basePath: "/rw-docs", onNavigate });
    const cleanup = router.initRouter(rootElement);

    const anchor = document.createElement("a");
    anchor.setAttribute("href", "/rw-docs/docs/guide");
    rootElement.appendChild(anchor);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    vi.spyOn(event, "preventDefault");
    Object.defineProperty(event, "target", { value: anchor });

    // Dispatch through the rootElement's click handler
    rootElement.dispatchEvent(event);

    expect(event.preventDefault).toHaveBeenCalled();
    expect(onNavigate).toHaveBeenCalledWith("/docs/guide");
    expect(router.path).toBe("/docs/guide");

    cleanup();
  });

  it("click handler handles root basePath href", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, basePath: "/rw-docs", onNavigate });
    const cleanup = router.initRouter(rootElement);

    const anchor = document.createElement("a");
    anchor.setAttribute("href", "/rw-docs");
    rootElement.appendChild(anchor);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    vi.spyOn(event, "preventDefault");
    Object.defineProperty(event, "target", { value: anchor });

    rootElement.dispatchEvent(event);

    expect(onNavigate).toHaveBeenCalledWith("/");
    expect(router.path).toBe("/");

    cleanup();
  });
});
