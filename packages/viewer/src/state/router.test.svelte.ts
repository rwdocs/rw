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

  it("decodes percent-encoded hash", () => {
    const router = new Router();
    router.goto("/page#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82");

    expect(router.hash).toBe("привет");
  });

  it("clears hash store when hash is not present", () => {
    const router = new Router();
    router.goto("/new-path");

    expect(router.hash).toBe("");
  });

  // Scroll-to-top on navigation is handled by Layout component
  // which scrolls the actual content container element
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

    it("decodes percent-encoded hash on popstate", () => {
      Object.defineProperty(window, "location", {
        value: { pathname: "/page", hash: "#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82" },
        writable: true,
        configurable: true,
      });

      popstateHandler!({} as PopStateEvent);

      expect(router.hash).toBe("привет");
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

    function createSvgAnchor(
      href: string,
      options: { xlink?: boolean; attributes?: Record<string, string> } = {},
    ): SVGElement {
      const SVG_NS = "http://www.w3.org/2000/svg";
      const XLINK_NS = "http://www.w3.org/1999/xlink";

      const svg = document.createElementNS(SVG_NS, "svg");
      const anchor = document.createElementNS(SVG_NS, "a");
      if (options.xlink) {
        anchor.setAttributeNS(XLINK_NS, "xlink:href", href);
      } else {
        anchor.setAttribute("href", href);
      }
      for (const [key, value] of Object.entries(options.attributes ?? {})) {
        anchor.setAttribute(key, value);
      }
      const text = document.createElementNS(SVG_NS, "text");
      anchor.appendChild(text);
      svg.appendChild(anchor);
      return text;
    }

    it("navigates on SVG link with target attribute (e.g., PlantUML diagrams)", () => {
      const text = createSvgAnchor("/domains/billing", { attributes: { target: "_top" } });
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: text });

      clickHandler!(event);

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/domains/billing");
    });

    it("navigates on SVG link with xlink:href", () => {
      const text = createSvgAnchor("/domains/billing", { xlink: true });
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: text });

      clickHandler!(event);

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/domains/billing");
    });

    it("ignores SVG links with external URLs", () => {
      const text = createSvgAnchor("https://external.com");
      const event = createClickEvent();
      Object.defineProperty(event, "target", { value: text });

      clickHandler!(event);

      expect(event.preventDefault).not.toHaveBeenCalled();
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

  it("decodes percent-encoded hash from initialPath in embedded mode", () => {
    const router = new Router({
      embedded: true,
      initialPath: "/guide#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82",
    });

    expect(router.hash).toBe("привет");
  });

  it("initializes hash to empty when initialPath has no hash in embedded mode", () => {
    const router = new Router({ embedded: true, initialPath: "/guide" });

    expect(router.hash).toBe("");
  });

  it("decodes percent-encoded hash from window.location in normal mode", () => {
    Object.defineProperty(window, "location", {
      value: {
        origin: "http://localhost:8001",
        pathname: "/page",
        hash: "#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82",
      },
      writable: true,
      configurable: true,
    });

    const router = new Router();

    expect(router.hash).toBe("привет");
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
    router.setBasePath("");
    router.goto("/guide");

    expect(window.history.pushState).not.toHaveBeenCalled();
    expect(router.path).toBe("/guide");
  });

  it("goto calls onNavigate callback in embedded mode", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("");
    router.goto("/guide");

    expect(onNavigate).toHaveBeenCalledWith("/guide");
    expect(window.history.pushState).not.toHaveBeenCalled();
  });

  it("goto calls onNavigate with basePath-prefixed path", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
    router.goto("/guide");

    expect(onNavigate).toHaveBeenCalledWith("/rw-docs/guide");
    expect(router.path).toBe("/guide");
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
    const router = new Router({ embedded: true });
    router.setBasePath("/rw-docs");
    expect(router.prefixPath("/docs/guide")).toBe("/rw-docs/docs/guide");
  });

  it("prefixPath handles root path", () => {
    const router = new Router({ embedded: true });
    router.setBasePath("/rw-docs");
    expect(router.prefixPath("/")).toBe("/rw-docs/");
  });

  it("click handler strips basePath from href before navigating", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
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
    expect(onNavigate).toHaveBeenCalledWith("/rw-docs/docs/guide");
    expect(router.path).toBe("/docs/guide");

    cleanup();
  });

  it("click handler handles root basePath href", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
    const cleanup = router.initRouter(rootElement);

    const anchor = document.createElement("a");
    anchor.setAttribute("href", "/rw-docs");
    rootElement.appendChild(anchor);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    vi.spyOn(event, "preventDefault");
    Object.defineProperty(event, "target", { value: anchor });

    rootElement.dispatchEvent(event);

    expect(onNavigate).toHaveBeenCalledWith("/rw-docs/");
    expect(router.path).toBe("/");

    cleanup();
  });

  it("click handler intercepts cross-section links and calls onNavigate with href", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/catalog/default/domain/billing/docs");
    const cleanup = router.initRouter(rootElement);

    const anchor = document.createElement("a");
    anchor.setAttribute("href", "/catalog/default/system/payment-gateway/docs");
    rootElement.appendChild(anchor);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    vi.spyOn(event, "preventDefault");
    Object.defineProperty(event, "target", { value: anchor });

    rootElement.dispatchEvent(event);

    expect(event.preventDefault).toHaveBeenCalled();
    expect(onNavigate).toHaveBeenCalledWith("/catalog/default/system/payment-gateway/docs");
    // Internal path should NOT change — viewer is navigating away
    expect(router.path).toBe("/");

    cleanup();
  });

  it("click handler falls through on cross-section links when no onNavigate", () => {
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true });
    router.setBasePath("/catalog/default/domain/billing/docs");
    const cleanup = router.initRouter(rootElement);

    const anchor = document.createElement("a");
    anchor.setAttribute("href", "/catalog/default/system/payment-gateway/docs");
    rootElement.appendChild(anchor);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    vi.spyOn(event, "preventDefault");
    Object.defineProperty(event, "target", { value: anchor });

    rootElement.dispatchEvent(event);

    expect(event.preventDefault).not.toHaveBeenCalled();

    cleanup();
  });
});

describe("scopePath", () => {
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

  it("prefixPath strips scopePath before prepending basePath", () => {
    const router = new Router({ embedded: true });
    router.setBasePath("/catalog/default/system/payment-gateway/docs");
    router.setScopePath("/domains/billing/systems/payment-gateway");
    expect(router.prefixPath("/domains/billing/systems/payment-gateway/usecases/get-cards")).toBe(
      "/catalog/default/system/payment-gateway/docs/usecases/get-cards",
    );
  });

  it("prefixPath handles scope root path", () => {
    const router = new Router({ embedded: true });
    router.setBasePath("/catalog/default/system/payment-gateway/docs");
    router.setScopePath("/domains/billing/systems/payment-gateway");
    expect(router.prefixPath("/domains/billing/systems/payment-gateway")).toBe(
      "/catalog/default/system/payment-gateway/docs/",
    );
  });

  it("prefixPath leaves paths outside scope unchanged", () => {
    const router = new Router({ embedded: true });
    router.setBasePath("/catalog/default/system/payment-gateway/docs");
    router.setScopePath("/domains/billing/systems/payment-gateway");
    expect(router.prefixPath("/other/path")).toBe(
      "/catalog/default/system/payment-gateway/docs/other/path",
    );
  });

  it("initialPath is scope-relative and gets scope prefix added", () => {
    const router = new Router({
      embedded: true,
      initialPath: "/usecases/get-cards",
    });
    router.setScopePath("/domains/billing/systems/payment-gateway");
    expect(router.path).toBe("/domains/billing/systems/payment-gateway/usecases/get-cards");
  });

  it("initialPath root maps to scopePath", () => {
    const router = new Router({
      embedded: true,
      initialPath: "/",
    });
    router.setScopePath("/domains/billing/systems/payment-gateway");
    expect(router.path).toBe("/domains/billing/systems/payment-gateway");
  });

  it("goto calls onNavigate with scope-stripped href", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/catalog/default/system/payment-gateway/docs");
    router.setScopePath("/domains/billing/systems/payment-gateway");
    router.goto("/domains/billing/systems/payment-gateway/usecases/get-cards");

    expect(onNavigate).toHaveBeenCalledWith(
      "/catalog/default/system/payment-gateway/docs/usecases/get-cards",
    );
    expect(router.path).toBe("/domains/billing/systems/payment-gateway/usecases/get-cards");
  });

  it("click handler adds scope prefix when converting URL to internal path", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/catalog/default/system/payment-gateway/docs");
    router.setScopePath("/domains/billing/systems/payment-gateway");
    const cleanup = router.initRouter(rootElement);

    const anchor = document.createElement("a");
    anchor.setAttribute("href", "/catalog/default/system/payment-gateway/docs/usecases/get-cards");
    rootElement.appendChild(anchor);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    vi.spyOn(event, "preventDefault");
    Object.defineProperty(event, "target", { value: anchor });

    rootElement.dispatchEvent(event);

    expect(event.preventDefault).toHaveBeenCalled();
    expect(router.path).toBe("/domains/billing/systems/payment-gateway/usecases/get-cards");
    expect(onNavigate).toHaveBeenCalledWith(
      "/catalog/default/system/payment-gateway/docs/usecases/get-cards",
    );

    cleanup();
  });
});

describe("two-phase initialization", () => {
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

  it("basePath starts empty and can be set", () => {
    const router = new Router({ embedded: true });
    expect(router.getBasePath()).toBe("");
    router.setBasePath("/catalog/default/domain/billing/docs");
    expect(router.getBasePath()).toBe("/catalog/default/domain/billing/docs");
  });

  it("scopePath starts empty and can be set", () => {
    const router = new Router({ embedded: true });
    expect(router.getScopePath()).toBe("");
    router.setScopePath("/domains/billing");
    expect(router.getScopePath()).toBe("/domains/billing");
  });

  it("resolved is false until basePath is set", () => {
    const router = new Router({ embedded: true });
    expect(router.resolved).toBe(false);
    router.setBasePath("/docs");
    expect(router.resolved).toBe(true);
  });

  it("prefixPath uses basePath after resolution", () => {
    const router = new Router({ embedded: true });
    router.setScopePath("/domains/billing");
    router.setBasePath("/catalog/default/domain/billing/docs");
    expect(router.prefixPath("/domains/billing/api")).toBe(
      "/catalog/default/domain/billing/docs/api",
    );
  });

  it("setScopePath adjusts current path with addScope", () => {
    const router = new Router({ embedded: true, initialPath: "/api/overview" });
    expect(router.path).toBe("/api/overview");
    router.setScopePath("/domains/billing");
    expect(router.path).toBe("/domains/billing/api/overview");
  });

  it("setScopePath adjusts root initialPath", () => {
    const router = new Router({ embedded: true, initialPath: "/" });
    router.setScopePath("/domains/billing");
    expect(router.path).toBe("/domains/billing");
  });

  it("goto queues navigation before resolution, replays after", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.goto("/some/path");
    expect(onNavigate).not.toHaveBeenCalled();
    router.setBasePath("/docs");
    expect(onNavigate).toHaveBeenCalledWith("/docs/some/path");
  });
});
