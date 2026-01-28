import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { get } from "svelte/store";
import { extractDocPath, goto, initRouter, path, hash } from "./router";

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
      value: { origin: "http://localhost:8001" },
      writable: true,
      configurable: true,
    });
    vi.spyOn(window.history, "pushState").mockImplementation(() => {});
    vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("pushes new path to history", () => {
    goto("/new-path");

    expect(window.history.pushState).toHaveBeenCalledWith({}, "", "/new-path");
  });

  it("updates path store", () => {
    goto("/new-path");

    expect(get(path)).toBe("/new-path");
  });

  it("updates hash store when hash is present", () => {
    goto("/new-path#section");

    expect(get(path)).toBe("/new-path");
    expect(get(hash)).toBe("section");
  });

  it("clears hash store when hash is not present", () => {
    goto("/new-path");

    expect(get(hash)).toBe("");
  });

  it("scrolls to top when no hash", () => {
    goto("/new-path");

    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
  });

  it("does not scroll when hash is present", () => {
    goto("/new-path#section");

    expect(window.scrollTo).not.toHaveBeenCalled();
  });
});

describe("initRouter", () => {
  let popstateHandler: ((e: PopStateEvent) => void) | null = null;
  let clickHandler: ((e: MouseEvent) => void) | null = null;

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
    vi.spyOn(window, "scrollTo").mockImplementation(() => {});

    initRouter();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    popstateHandler = null;
    clickHandler = null;
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

      expect(get(path)).toBe("/back-path");
      expect(get(hash)).toBe("");
    });

    it("updates hash store when navigating to URL with hash", () => {
      Object.defineProperty(window, "location", {
        value: { pathname: "/back-path", hash: "#section" },
        writable: true,
        configurable: true,
      });

      popstateHandler!({} as PopStateEvent);

      expect(get(path)).toBe("/back-path");
      expect(get(hash)).toBe("section");
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
      expect(get(path)).toBe("/internal-page");
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
      expect(get(path)).toBe("/nested-page");
    });
  });
});
