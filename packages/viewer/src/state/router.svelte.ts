import { on } from "svelte/events";

function isExternalLink(href: string, anchor: Element): boolean {
  if (
    href.startsWith("http") ||
    href.startsWith("//") ||
    href.startsWith("mailto:") ||
    href.startsWith("tel:") ||
    href.startsWith("#")
  ) {
    return true;
  }

  // SVG links (e.g., PlantUML diagrams) add target="_top" — ignore element
  // attributes for SVG anchors so internal diagram links use SPA routing.
  if (anchor instanceof SVGElement) return false;

  return anchor.hasAttribute("target") || anchor.hasAttribute("download");
}

function decode(encoded: string): string {
  try {
    return decodeURIComponent(encoded);
  } catch {
    return encoded;
  }
}

export class Router {
  /** Decoded, as the server names pages: `/гайд`, not `/%D0%B3%D0%B0%D0%B9%D0%B4`. */
  path = $state("/");
  hash = $state("");
  /** Standalone needs no basePath from the host, so only embedded starts unresolved. */
  resolved = $state(false);
  readonly embedded: boolean;
  private basePath = $state("");
  private scopePath = $state("");
  private readonly onNavigate?: (path: string) => void;
  private pendingGotos: string[] = [];
  private scopeApplied = false;

  constructor(options?: {
    embedded?: boolean;
    initialPath?: string;
    onNavigate?: (path: string) => void;
  }) {
    this.embedded = options?.embedded ?? false;
    this.onNavigate = options?.onNavigate;
    this.resolved = !this.embedded;

    if (this.embedded) {
      this.path = decode(options?.initialPath?.split("#")[0] ?? "/");
      this.hash = decode(options?.initialPath?.split("#")[1] ?? "");
    } else {
      this.path = decode(window.location.pathname);
      this.hash = decode(window.location.hash.slice(1));
    }
  }

  getBasePath(): string {
    return this.basePath;
  }

  setBasePath(value: string): void {
    this.basePath = value;
    this.resolved = true;

    const pending = this.pendingGotos.splice(0);
    for (const path of pending) {
      this.goto(path);
    }
  }

  setScopePath(value: string): void {
    this.scopePath = value;

    if (this.embedded && !this.scopeApplied) {
      this.scopeApplied = true;
      this.path = this.addScope(this.path);
    }
  }

  /** Strip scopePath prefix from an internal path for URL construction. */
  private stripScope(path: string): string {
    if (!this.scopePath) return path;
    if (path === this.scopePath) return "/";
    if (path.startsWith(this.scopePath + "/")) return path.slice(this.scopePath.length);
    return path;
  }

  /** Add scopePath prefix to a URL path for internal/API use. */
  private addScope(path: string): string {
    if (!this.scopePath) return path;
    if (path === "/") return this.scopePath;
    return this.scopePath + path;
  }

  /** Prefix an internal path with basePath for use in href attributes. */
  prefixPath = (path: string): string => {
    return this.basePath + this.stripScope(path);
  };

  /** Link target for a nav item or breadcrumb: its resolved cross-section `href`, else its prefixed path. */
  hrefFor = (target: { path: string; href?: string }): string => {
    return target.href ?? this.prefixPath(target.path);
  };

  goto = (newPath: string) => {
    if (!this.resolved) {
      this.pendingGotos.push(newPath);
      return;
    }

    // pushState resolves a relative path against the current URL; match it.
    const url = new URL(newPath, window.location.href);

    if (!this.embedded) {
      window.history.pushState({}, "", newPath);
    } else if (this.onNavigate) {
      this.onNavigate(this.prefixPath(newPath));
    }

    this.path = decode(url.pathname);
    this.hash = decode(url.hash.slice(1));
  };

  /** Call once on mount. In embedded mode, pass the app's root element so only
   *  clicks inside the viewer are routed, not the host's. */
  initRouter = (rootElement?: HTMLElement): (() => void) => {
    const handlePopState = () => {
      this.path = decode(window.location.pathname);
      this.hash = decode(window.location.hash.slice(1));
    };

    const handleClick = (e: MouseEvent) => {
      // `composedPath()` rather than `closest()`: a click inside a diagram's
      // shadow root retargets `e.target` to the <rw-diagram> host, so the
      // anchor is only reachable through the composed path.
      const anchor = e
        .composedPath()
        .find((n): n is Element => n instanceof Element && n.matches("a"));
      if (!anchor) return;

      const href =
        anchor.getAttribute("href") ??
        anchor.getAttributeNS("http://www.w3.org/1999/xlink", "href");
      if (!href) return;

      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      if (isExternalLink(href, anchor)) return;

      if (!this.resolved) return;

      // The renderer leaves non-`.md` relative links as written; in embedded
      // mode `baseURI` is the host page, which maps back through basePath.
      const url = new URL(href, anchor.baseURI);
      if (url.origin !== window.location.origin) return;
      const { pathname, search, hash } = url;
      // The host may hand over basePath decoded; `pathname` is always encoded.
      const basePath = this.basePath && new URL(this.basePath, url).pathname;

      const inSection = !basePath || pathname === basePath || pathname.startsWith(basePath + "/");

      // Cross-section link — let host handle routing via onNavigate
      if (!inSection) {
        if (this.onNavigate) {
          e.preventDefault();
          this.onNavigate(pathname + search + hash);
        }
        return;
      }

      const urlPath = pathname.slice(basePath.length) || "/";
      // Joins the decoded scope to the encoded urlPath; `goto` parses the
      // mix into one encoded pathname before decoding it.
      const internalPath = this.addScope(urlPath) + search + hash;

      e.preventDefault();
      this.goto(internalPath);
    };

    const clickTarget: Document | HTMLElement =
      this.embedded && rootElement ? rootElement : document;

    const offPopState = this.embedded ? null : on(window, "popstate", handlePopState);
    const offClick = on(clickTarget, "click", handleClick as EventListener);

    return () => {
      offPopState?.();
      offClick();
    };
  };
}
