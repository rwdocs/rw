import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Router } from "./router.svelte";
import { registerRwDiagram } from "../lib/diagram/rwDiagramElement";

// Moves jsdom's real URL, so `window.location` and anchors' `baseURI` agree.
function setLocation(url: string): void {
  window.history.replaceState(null, "", url);
}

/** An embedded router past the navigation load that resolves it. */
function resolvedAt(initialPath: string | undefined, scope = "/"): Router {
  const router = new Router({ embedded: true, initialPath });
  router.setScopePath(scope);
  router.setBasePath("/docs");
  return router;
}

function clickLinkIn(rootElement: HTMLElement, href: string): MouseEvent {
  const anchor = document.createElement("a");
  anchor.setAttribute("href", href);
  rootElement.appendChild(anchor);
  const event = new MouseEvent("click", { bubbles: true, cancelable: true });
  vi.spyOn(event, "preventDefault");
  anchor.dispatchEvent(event);
  return event;
}

describe("goto", () => {
  beforeEach(() => {
    setLocation("/");
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("pushes the path to history and clears the hash", () => {
    const router = new Router();
    router.goto("/new-path");

    expect(window.history.pushState).toHaveBeenCalledWith({}, "", "/new-path");
    expect(router.path).toBe("/new-path");
    expect(router.hash).toBe("");
  });

  it("splits the hash off the path", () => {
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

  it("resolves a relative path against the current page, as pushState does", () => {
    setLocation("/a/b");
    const router = new Router();
    router.goto("c");

    expect(router.path).toBe("/a/c");
  });
});

describe("initRouter", () => {
  let popstateHandler: ((e: PopStateEvent) => void) | null = null;
  let clickHandler: ((e: MouseEvent) => void) | null = null;
  let cleanup: (() => void) | null = null;
  let router: Router;

  beforeEach(() => {
    setLocation("/");
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

  describe("popstate handler", () => {
    it("updates path store on back/forward navigation", () => {
      setLocation("/back-path");

      popstateHandler!(new PopStateEvent("popstate"));

      expect(router.path).toBe("/back-path");
      expect(router.hash).toBe("");
    });

    it("updates hash store when navigating to URL with hash", () => {
      setLocation("/back-path#section");

      popstateHandler!(new PopStateEvent("popstate"));

      expect(router.path).toBe("/back-path");
      expect(router.hash).toBe("section");
    });

    it("decodes percent-encoded hash on popstate", () => {
      setLocation("/page#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82");

      popstateHandler!(new PopStateEvent("popstate"));

      expect(router.hash).toBe("привет");
    });
  });

  describe("click handler", () => {
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

    // `clickHandler` is called directly, never dispatched, so `composedPath()`
    // would be empty; rebuild it from the target's ancestor chain.
    function click(target: Element, modifiers: MouseEventInit = {}): MouseEvent {
      const event = new MouseEvent("click", { bubbles: true, cancelable: true, ...modifiers });
      vi.spyOn(event, "preventDefault");
      Object.defineProperty(event, "target", { value: target, configurable: true });
      const path: Element[] = [];
      for (let node: Element | null = target; node; node = node.parentElement) {
        path.push(node);
      }
      vi.spyOn(event, "composedPath").mockReturnValue(path);
      clickHandler!(event);
      return event;
    }

    it("navigates on internal link click", () => {
      const event = click(createAnchor("/internal-page"));

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/internal-page");
    });

    it.each([
      { from: "/a/b", href: "c", path: "/a/c", pushed: "/a/c" },
      { from: "/a/b/c", href: "../x", path: "/a/x", pushed: "/a/x" },
      { from: "/a/b", href: "?tab=2", path: "/a/b", pushed: "/a/b?tab=2" },
    ])("resolves relative $href against $from", ({ from, href, path, pushed }) => {
      setLocation(from);

      const event = click(createAnchor(href));

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe(path);
      expect(window.history.pushState).toHaveBeenCalledWith({}, "", pushed);
    });

    it("ignores links to another origin", () => {
      const event = click(createAnchor("ftp://example.com/file"));

      expect(event.preventDefault).not.toHaveBeenCalled();
      expect(router.path).toBe("/");
    });

    it("handles clicks on nested elements inside anchor", () => {
      const anchor = createAnchor("/nested-page");
      const span = document.createElement("span");
      anchor.appendChild(span);

      const event = click(span);

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/nested-page");
    });

    it("ignores clicks not on anchors", () => {
      const event = click(document.createElement("div"));

      expect(event.preventDefault).not.toHaveBeenCalled();
    });

    it.each(["metaKey", "ctrlKey", "shiftKey", "altKey"] as const)(
      "ignores clicks with %s held",
      (modifier) => {
        const event = click(createAnchor("/page"), { [modifier]: true });

        expect(event.preventDefault).not.toHaveBeenCalled();
        expect(router.path).toBe("/");
      },
    );

    it.each<{ name: string; href: string | null; attributes?: Record<string, string> }>([
      { name: "anchors without href", href: null },
      { name: "external http links", href: "https://example.com" },
      { name: "protocol-relative links", href: "//example.com/page" },
      { name: "mailto links", href: "mailto:test@example.com" },
      { name: "tel links", href: "tel:+1234567890" },
      { name: "hash links", href: "#section" },
      { name: "links with target attribute", href: "/page", attributes: { target: "_blank" } },
      { name: "links with download attribute", href: "/file.pdf", attributes: { download: "" } },
    ])("ignores $name", ({ href, attributes }) => {
      const event = click(createAnchor(href, attributes));

      expect(event.preventDefault).not.toHaveBeenCalled();
      expect(router.path).toBe("/");
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
      const event = click(createSvgAnchor("/domains/billing", { attributes: { target: "_top" } }));

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/domains/billing");
    });

    it("navigates on SVG link with xlink:href", () => {
      const event = click(createSvgAnchor("/domains/billing", { xlink: true }));

      expect(event.preventDefault).toHaveBeenCalled();
      expect(router.path).toBe("/domains/billing");
    });

    it("ignores SVG links with external URLs", () => {
      const event = click(createSvgAnchor("https://external.com"));

      expect(event.preventDefault).not.toHaveBeenCalled();
    });
  });
});

describe("embedded mode", () => {
  beforeEach(() => {
    setLocation("/");
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("uses initialPath as starting path in embedded mode", () => {
    const router = resolvedAt("/guide");

    expect(router.path).toBe("/guide");
    expect(router.hash).toBe("");
  });

  it("parses hash from initialPath in embedded mode", () => {
    const router = resolvedAt("/guide#section");

    expect(router.path).toBe("/guide");
    expect(router.hash).toBe("section");
  });

  it("decodes percent-encoded hash from initialPath in embedded mode", () => {
    const router = resolvedAt("/guide#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82");

    expect(router.hash).toBe("привет");
  });

  it("decodes percent-encoded hash from window.location in normal mode", () => {
    setLocation("/page#%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82");

    const router = new Router();

    expect(router.hash).toBe("привет");
  });

  it("ignores initialPath in normal mode", () => {
    const router = new Router({ embedded: false, initialPath: "/guide" });

    expect(router.path).toBe("/");
  });

  it("defaults to / when no initialPath in embedded mode", () => {
    setLocation("/backstage/catalog");

    const router = resolvedAt(undefined);

    // Should default to "/" not the host app's pathname
    expect(router.path).toBe("/");
  });

  it("goto calls onNavigate instead of pushState in embedded mode", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("");
    router.goto("/guide");

    expect(onNavigate).toHaveBeenCalledWith("/guide");
    expect(window.history.pushState).not.toHaveBeenCalled();
    expect(router.path).toBe("/guide");
  });

  it("goto calls onNavigate with basePath-prefixed path", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
    router.goto("/guide");

    expect(onNavigate).toHaveBeenCalledWith("/rw-docs/guide");
    expect(router.path).toBe("/guide");
  });

  // Like a controlled input: the host hears about navigation the viewer starts,
  // not navigation it asked for, so a host that pushes history in `onNavigate`
  // keeps Forward after Back.
  it("reports a link click to onNavigate, but not gotoHostPath", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
    const cleanup = router.initRouter(rootElement);

    router.gotoHostPath("/guide#intro");
    expect(router.path).toBe("/guide");
    expect(router.hash).toBe("intro");
    expect(onNavigate).not.toHaveBeenCalled();

    clickLinkIn(rootElement, "/rw-docs/setup");
    expect(router.path).toBe("/setup");
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith("/rw-docs/setup");

    cleanup();
  });

  it("takes over an in-page anchor click, keeping history.state", () => {
    const hostState = { usr: null, key: "k1", idx: 3 };
    window.history.replaceState(hostState, "");
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "#%D1%88%D0%B0%D0%B3");

    expect(event.preventDefault).toHaveBeenCalled();
    expect(window.history.pushState).toHaveBeenCalledExactlyOnceWith(hostState, "", "#шаг");
    expect(router.hash).toBe("шаг");
    expect(onNavigate).not.toHaveBeenCalled();

    cleanup();
  });

  it("goto does not call onNavigate in normal mode", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: false, onNavigate });
    router.goto("/guide");

    expect(onNavigate).not.toHaveBeenCalled();
    expect(window.history.pushState).toHaveBeenCalled();
  });

  it("initRouter skips popstate listener in embedded mode", () => {
    const addEventSpy = vi.spyOn(window, "addEventListener");
    const router = new Router({ embedded: true });
    const cleanup = router.initRouter();

    const popstateCall = addEventSpy.mock.calls.find(([event]) => event === "popstate");
    expect(popstateCall).toBeUndefined();

    cleanup();
  });

  it("initRouter registers popstate listener in normal mode", () => {
    const addEventSpy = vi.spyOn(window, "addEventListener");
    const router = new Router({ embedded: false });
    const cleanup = router.initRouter();

    const popstateCall = addEventSpy.mock.calls.find(([event]) => event === "popstate");
    expect(popstateCall).toBeDefined();

    cleanup();
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
  });

  it("initRouter falls back to document when no root element in embedded mode", () => {
    const docAddEventSpy = vi.spyOn(document, "addEventListener");

    const router = new Router({ embedded: true });
    const cleanup = router.initRouter();

    const docClickCall = docAddEventSpy.mock.calls.find(([event]) => event === "click");
    expect(docClickCall).toBeDefined();

    cleanup();
  });
});

describe("basePath", () => {
  beforeEach(() => {
    setLocation("/");
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

  it("hrefFor prefers a resolved href over the prefixed path", () => {
    const router = new Router({ embedded: true });
    router.setBasePath("/rw-docs");
    expect(router.hrefFor({ path: "/guide" })).toBe("/rw-docs/guide");
    expect(router.hrefFor({ path: "/guide", href: "/catalog/guide" })).toBe("/catalog/guide");
  });

  it("click handler strips basePath from href before navigating", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "/rw-docs/docs/guide");

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

    clickLinkIn(rootElement, "/rw-docs");

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

    const event = clickLinkIn(rootElement, "/catalog/default/system/payment-gateway/docs");

    expect(event.preventDefault).toHaveBeenCalled();
    expect(onNavigate).toHaveBeenCalledWith("/catalog/default/system/payment-gateway/docs");
    // Internal path should NOT change — viewer is navigating away
    expect(router.path).toBe("/");

    cleanup();
  });

  it("click handler resolves a relative href against the host page", () => {
    setLocation("/rw-docs/guide/intro");
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/rw-docs");
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "setup");

    expect(event.preventDefault).toHaveBeenCalled();
    expect(onNavigate).toHaveBeenCalledWith("/rw-docs/guide/setup");
    expect(router.path).toBe("/guide/setup");

    cleanup();
  });

  it("click handler treats a sibling of basePath as cross-section", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/x/docs");
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "/x/docs-v2/page");

    expect(event.preventDefault).toHaveBeenCalled();
    expect(onNavigate).toHaveBeenCalledWith("/x/docs-v2/page");
    expect(router.path).toBe("/");

    cleanup();
  });

  it("click handler falls through on cross-section links when no onNavigate", () => {
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true });
    router.setBasePath("/catalog/default/domain/billing/docs");
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "/catalog/default/system/payment-gateway/docs");

    expect(event.preventDefault).not.toHaveBeenCalled();

    cleanup();
  });
});

describe("scopePath", () => {
  beforeEach(() => {
    setLocation("/");
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
    const router = resolvedAt("/usecases/get-cards", "/domains/billing/systems/payment-gateway");
    expect(router.path).toBe("/domains/billing/systems/payment-gateway/usecases/get-cards");
  });

  it("initialPath root maps to scopePath", () => {
    const router = resolvedAt("/", "/domains/billing/systems/payment-gateway");
    expect(router.path).toBe("/domains/billing/systems/payment-gateway");
  });

  it("treats the root scope as no scope", () => {
    const router = new Router({ embedded: true, initialPath: "/guide" });
    router.setScopePath("/");
    router.setBasePath("/catalog/x/docs");

    expect(router.path).toBe("/guide");
    expect(router.prefixPath("/guide")).toBe("/catalog/x/docs/guide");
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

  it.each(["#intro", "?tab=api"])("goto maps the scope root with %j onto basePath", (suffix) => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/catalog/x/docs");
    router.setScopePath("/billing");
    router.goto(`/billing${suffix}`);

    expect(onNavigate).toHaveBeenCalledWith(`/catalog/x/docs/${suffix}`);
    expect(router.path).toBe("/billing");
  });

  it("click handler adds scope prefix when converting URL to internal path", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/catalog/default/system/payment-gateway/docs");
    router.setScopePath("/domains/billing/systems/payment-gateway");
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(
      rootElement,
      "/catalog/default/system/payment-gateway/docs/usecases/get-cards",
    );

    expect(event.preventDefault).toHaveBeenCalled();
    expect(router.path).toBe("/domains/billing/systems/payment-gateway/usecases/get-cards");
    expect(onNavigate).toHaveBeenCalledWith(
      "/catalog/default/system/payment-gateway/docs/usecases/get-cards",
    );

    cleanup();
  });

  it("click handler scopes a relative href resolved against the host page", () => {
    setLocation("/guide/intro");
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("");
    router.setScopePath("/scope");
    const cleanup = router.initRouter(rootElement);

    clickLinkIn(rootElement, "setup");

    expect(router.path).toBe("/scope/guide/setup");
    expect(onNavigate).toHaveBeenCalledWith("/guide/setup");

    cleanup();
  });
});

describe("diagram links", () => {
  beforeEach(() => {
    setLocation("/");
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("routes a click on a link inside a diagram shadow root", () => {
    registerRwDiagram();
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.innerHTML = `<rw-diagram><svg><a href="/guide">x</a></svg></rw-diagram>`;
    const anchor = host.querySelector("rw-diagram")!.shadowRoot!.querySelector("a")!;

    const router = new Router();
    const cleanup = router.initRouter();

    anchor.dispatchEvent(
      new MouseEvent("click", { bubbles: true, composed: true, cancelable: true }),
    );

    expect(router.path).toBe("/guide");

    cleanup();
    host.remove();
  });
});

describe("two-phase initialization", () => {
  beforeEach(() => {
    setLocation("/");
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("is unresolved until setBasePath supplies the base", () => {
    const router = new Router({ embedded: true });
    expect(router.resolved).toBe(false);
    router.setBasePath("/docs");
    expect(router.resolved).toBe(true);
    expect(router.getBasePath()).toBe("/docs");
  });

  it("gotoHostPath queues navigation before resolution, replays it in the scope", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.gotoHostPath("/some/path");
    expect(router.path).toBe("/");
    router.setScopePath("/billing");
    router.setBasePath("/docs");
    expect(router.path).toBe("/billing/some/path");
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("parses initialPath as navigateTo parses the same path", () => {
    const initial = resolvedAt("/guide?tab=x#intro");
    const navigated = resolvedAt("/");
    navigated.gotoHostPath("/guide?tab=x#intro");

    expect(initial.path).toBe("/guide");
    expect(initial.hash).toBe("intro");
    expect([initial.path, initial.hash]).toEqual([navigated.path, navigated.hash]);
  });

  it("ignores link clicks before resolution", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "/guide");

    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(onNavigate).not.toHaveBeenCalled();

    cleanup();
  });
});

// The server sends nav, live-reload, and scope paths as raw UTF-8, so the
// router must hold the decoded form to compare equal.
describe("non-ASCII paths", () => {
  beforeEach(() => {
    setLocation("/");
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("decodes the initial location", () => {
    setLocation("/гайд/шаг");

    expect(new Router().path).toBe("/гайд/шаг");
  });

  it.each(["/гайд", "/%D0%B3%D0%B0%D0%B9%D0%B4"])("goto(%j) stores the decoded path", (path) => {
    const router = new Router();
    router.goto(path);

    expect(router.path).toBe("/гайд");
  });

  it("decodes the location on popstate", () => {
    const router = new Router();
    const cleanup = router.initRouter();
    setLocation("/гайд/шаг");

    window.dispatchEvent(new PopStateEvent("popstate"));

    expect(router.path).toBe("/гайд/шаг");
    cleanup();
  });

  it("decodes a clicked link", () => {
    const root = document.body.appendChild(document.createElement("div"));
    const router = new Router();
    const cleanup = router.initRouter();

    clickLinkIn(root, "/гайд/шаг");

    expect(router.path).toBe("/гайд/шаг");
    cleanup();
    root.remove();
  });

  it.each(["/%E0%A4%A", "/100%"])("keeps the malformed escape in %j as is", (path) => {
    setLocation(path);
    const router = new Router();

    expect(router.path).toBe(path);
    router.goto(path);
    expect(router.path).toBe(path);
  });

  it("decodes the valid escapes around a malformed one", () => {
    setLocation("/гайд/100%");

    expect(new Router().path).toBe("/гайд/100%");
  });

  it("decodes initialPath under a non-ASCII scope", () => {
    const router = resolvedAt("/%D1%88%D0%B0%D0%B3", "/гайд");

    expect(router.path).toBe("/гайд/шаг");
  });

  it("maps a clicked link back through a non-ASCII scope", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setScopePath("/гайд");
    router.setBasePath("/catalog/x/docs");
    const cleanup = router.initRouter(rootElement);

    const event = clickLinkIn(rootElement, "/catalog/x/docs/шаг");

    expect(event.preventDefault).toHaveBeenCalled();
    expect(router.path).toBe("/гайд/шаг");
    expect(onNavigate).toHaveBeenCalledWith("/catalog/x/docs/%D1%88%D0%B0%D0%B3");
    cleanup();
  });

  it.each(["/каталог/docs", "/%D0%BA%D0%B0%D1%82%D0%B0%D0%BB%D0%BE%D0%B3/docs"])(
    "treats a link under basePath %j as in-section",
    (basePath) => {
      const rootElement = document.createElement("div");
      const router = new Router({ embedded: true, onNavigate: vi.fn() });
      router.setBasePath(basePath);
      const cleanup = router.initRouter(rootElement);

      clickLinkIn(rootElement, "/каталог/docs/шаг");

      expect(router.path).toBe("/шаг");
      cleanup();
    },
  );
});

describe("reserved characters in page names", () => {
  beforeEach(() => {
    setLocation("/");
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const names = ["/a?b", "/c#d", "/e%f", "/x%41y", "/g h", "/гайд/шаг"];

  it("hrefFor encodes each segment but keeps basePath as the host gave it", () => {
    const router = new Router({ embedded: true });
    router.setBasePath("/каталог/docs");

    expect(router.hrefFor({ path: "/a?b/c#d/e%f/g h" })).toBe(
      "/каталог/docs/a%3Fb/c%23d/e%25f/g%20h",
    );
  });

  it.each(names)("a click on the href of %j stores that path", (path) => {
    const root = document.body.appendChild(document.createElement("div"));
    const router = new Router();
    const cleanup = router.initRouter();

    clickLinkIn(root, router.hrefFor({ path }));

    expect(router.path).toBe(path);
    expect(router.hash).toBe("");
    cleanup();
    root.remove();
  });

  it.each(names)("an embedded click on the href of %j stores that path", (path) => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setScopePath("/scope");
    router.setBasePath("/catalog/x/docs");
    const cleanup = router.initRouter(rootElement);
    const href = router.hrefFor({ path: `/scope${path}` });

    clickLinkIn(rootElement, href);

    expect(router.path).toBe(`/scope${path}`);
    expect(onNavigate).toHaveBeenCalledWith(href);
    cleanup();
  });

  it("maps a click back through a scope named with `?` and `#`", () => {
    const onNavigate = vi.fn();
    const rootElement = document.createElement("div");
    const router = new Router({ embedded: true, onNavigate });
    router.setScopePath("/a?b#c");
    router.setBasePath("/catalog/x/docs");
    const cleanup = router.initRouter(rootElement);

    clickLinkIn(rootElement, "/catalog/x/docs/page#intro");

    expect(router.path).toBe("/a?b#c/page");
    expect(router.hash).toBe("intro");
    expect(onNavigate).toHaveBeenCalledWith("/catalog/x/docs/page#intro");
    cleanup();
  });

  it("goto hands onNavigate an encoded path without encoding it twice", () => {
    const onNavigate = vi.fn();
    const router = new Router({ embedded: true, onNavigate });
    router.setBasePath("/catalog/x/docs");
    router.goto("/x%2541y/a%3Fb?tab=1#top");

    expect(onNavigate).toHaveBeenCalledWith("/catalog/x/docs/x%2541y/a%3Fb?tab=1#top");
    expect(router.path).toBe("/x%41y/a?b");
  });
});
