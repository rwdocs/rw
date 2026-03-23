/** Extract document path for API calls (strip leading slash) */
export function extractDocPath(urlPath: string): string {
  return urlPath.replace(/^\//, "");
}

/** Check if a link should be handled externally (not by SPA router) */
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

function decodeHash(encoded: string): string {
  try {
    return decodeURIComponent(encoded);
  } catch {
    return encoded;
  }
}

export class Router {
  path = $state("/");
  hash = $state("");
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

    if (this.embedded) {
      const raw = options?.initialPath?.split("#")[0] ?? "/";
      this.path = raw;
      this.hash = decodeHash(options?.initialPath?.split("#")[1] ?? "");
    } else {
      this.path = window.location.pathname;
      this.hash = decodeHash(window.location.hash.slice(1));
    }
  }

  getBasePath(): string {
    return this.basePath;
  }

  setBasePath(value: string): void {
    this.basePath = value;
    this.resolved = true;

    // Replay pending gotos
    const pending = this.pendingGotos.splice(0);
    for (const path of pending) {
      this.goto(path);
    }
  }

  getScopePath(): string {
    return this.scopePath;
  }

  setScopePath(value: string): void {
    this.scopePath = value;

    // Apply scope to current path once (embedded mode only)
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

  /** Navigate to a path programmatically */
  goto = (newPath: string) => {
    // Queue navigation until basePath is resolved in embedded mode
    if (!this.resolved && this.embedded) {
      this.pendingGotos.push(newPath);
      return;
    }

    const origin = typeof window !== "undefined" ? window.location.origin : "http://localhost";
    const url = new URL(newPath, origin);

    if (!this.embedded) {
      window.history.pushState({}, "", newPath);
    } else if (this.onNavigate) {
      this.onNavigate(this.basePath + this.stripScope(newPath));
    }

    this.path = url.pathname;
    this.hash = decodeHash(url.hash.slice(1));
  };

  /** Initialize router - call once on app mount. Returns cleanup function.
   * In embedded mode, pass the app's root element to scope click handling
   * to links within the RW app instead of the entire document. */
  initRouter = (rootElement?: HTMLElement): (() => void) => {
    const handlePopState = () => {
      this.path = window.location.pathname;
      this.hash = decodeHash(window.location.hash.slice(1));
    };

    const handleClick = (e: MouseEvent) => {
      const target = e.target;
      if (!(target instanceof Element)) return;

      const anchor = target.closest("a");
      if (!anchor) return;

      const href =
        anchor.getAttribute("href") ??
        anchor.getAttributeNS("http://www.w3.org/1999/xlink", "href");
      if (!href) return;

      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      if (isExternalLink(href, anchor)) return;

      // Not yet resolved in embedded mode — ignore clicks
      if (this.embedded && !this.resolved) return;

      // Cross-section link — let host handle routing via onNavigate
      if (this.basePath && !href.startsWith(this.basePath)) {
        if (this.onNavigate) {
          e.preventDefault();
          this.onNavigate(href);
        }
        return;
      }

      const urlPath = this.basePath ? href.slice(this.basePath.length) || "/" : href;
      const internalPath = this.addScope(urlPath);

      e.preventDefault();
      this.goto(internalPath);
    };

    const clickTarget: Document | HTMLElement =
      this.embedded && rootElement ? rootElement : document;

    if (!this.embedded) {
      window.addEventListener("popstate", handlePopState);
    }
    clickTarget.addEventListener("click", handleClick as EventListener);

    return () => {
      if (!this.embedded) {
        window.removeEventListener("popstate", handlePopState);
      }
      clickTarget.removeEventListener("click", handleClick as EventListener);
    };
  };
}
