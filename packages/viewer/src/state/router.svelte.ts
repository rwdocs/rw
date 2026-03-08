/** Extract document path for API calls (strip leading slash) */
export function extractDocPath(urlPath: string): string {
  return urlPath.replace(/^\//, "");
}

/** Check if a link should be handled externally (not by SPA router) */
function isExternalLink(href: string, anchor: HTMLAnchorElement): boolean {
  return (
    href.startsWith("http") ||
    href.startsWith("//") ||
    href.startsWith("mailto:") ||
    href.startsWith("tel:") ||
    href.startsWith("#") ||
    anchor.hasAttribute("target") ||
    anchor.hasAttribute("download")
  );
}

export class Router {
  path = $state("/");
  hash = $state("");
  readonly embedded: boolean;
  private readonly basePath: string;
  private readonly onNavigate?: (path: string) => void;
  private pathChangeListeners: Set<() => void> = new Set();

  constructor(options?: {
    embedded?: boolean;
    initialPath?: string;
    basePath?: string;
    onNavigate?: (path: string) => void;
  }) {
    this.embedded = options?.embedded ?? false;
    this.basePath = options?.basePath ?? "";
    this.onNavigate = options?.onNavigate;

    if (this.embedded) {
      this.path = options?.initialPath?.split("#")[0] ?? "/";
      this.hash = options?.initialPath?.split("#")[1] ?? "";
    } else {
      this.path = window.location.pathname;
      this.hash = window.location.hash.slice(1);
    }
  }

  /** Register a callback that fires on path changes. Returns an unsubscribe function. */
  onPathChange = (callback: () => void): (() => void) => {
    this.pathChangeListeners.add(callback);
    return () => this.pathChangeListeners.delete(callback);
  };

  private notifyPathChange = () => {
    for (const cb of this.pathChangeListeners) cb();
  };

  /** Prefix an internal path with basePath for use in href attributes. */
  prefixPath = (path: string): string => {
    return this.basePath + path;
  };

  /** Navigate to a path programmatically */
  goto = (newPath: string) => {
    const origin = typeof window !== "undefined" ? window.location.origin : "http://localhost";
    const url = new URL(newPath, origin);

    if (!this.embedded) {
      window.history.pushState({}, "", newPath);
    } else if (this.onNavigate) {
      this.onNavigate(newPath);
    }

    this.path = url.pathname;
    this.hash = url.hash.slice(1);
    this.notifyPathChange();
  };

  /** Initialize router - call once on app mount. Returns cleanup function.
   * In embedded mode, pass the app's root element to scope click handling
   * to links within the RW app instead of the entire document. */
  initRouter = (rootElement?: HTMLElement): (() => void) => {
    const handlePopState = () => {
      this.path = window.location.pathname;
      this.hash = window.location.hash.slice(1);
      this.notifyPathChange();
    };

    const handleClick = (e: MouseEvent) => {
      const target = e.target;
      if (!(target instanceof Element)) return;

      const anchor = target.closest("a");
      if (!anchor) return;

      const href = anchor.getAttribute("href");
      if (!href) return;

      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      if (isExternalLink(href, anchor)) return;

      const internalPath =
        this.basePath && href.startsWith(this.basePath)
          ? href.slice(this.basePath.length) || "/"
          : href;

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
