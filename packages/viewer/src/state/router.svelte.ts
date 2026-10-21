import { on } from "svelte/events";
import { encodePath } from "../lib/encodePath";

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

/** Decodes run by run, so one stray `%` does not keep the rest encoded. */
function decode(encoded: string): string {
  return encoded.replace(/(?:%[0-9A-F]{2})+/gi, (run) => {
    try {
      return decodeURIComponent(run);
    } catch {
      return run;
    }
  });
}

export class Router {
  /** Decoded, as the server names pages: `/гайд`, not `/%D0%B3%D0%B0%D0%B9%D0%B4`. */
  path = $state("/");
  hash = $state("");
  /** Embedded starts unresolved: a host location maps to a page only once the scope is known. */
  resolved = $state(false);
  readonly embedded: boolean;
  /** Where the host shows the scope root, without a trailing slash; "" for the origin root. */
  readonly basePath: string;
  private scopePath = $state("");
  private readonly onNavigate?: (path: string) => void;
  private pendingGotos: string[] = [];

  constructor(options?: {
    embedded?: boolean;
    basePath?: string;
    location?: string;
    onNavigate?: (path: string) => void;
  }) {
    this.embedded = options?.embedded ?? false;
    this.basePath = (options?.basePath ?? "").replace(/\/+$/, "");
    this.onNavigate = options?.onNavigate;
    this.resolved = !this.embedded;

    if (this.embedded) {
      this.pendingGotos.push(options?.location ?? (this.basePath || "/"));
    } else {
      this.path = decode(window.location.pathname);
      this.hash = decode(window.location.hash.slice(1));
    }
  }

  /** Resolves the router, so queued host locations go through now. */
  setScopePath(value: string): void {
    // The root scope "/" has no prefix to strip or add.
    this.scopePath = value === "/" ? "" : value;
    this.resolved = true;

    const [mount, ...later] = this.pendingGotos.splice(0);
    if (mount !== undefined) {
      // Ignoring the mount location would leave the path at "/", the site root.
      const scopeRoot = this.addScope("/", encodePath(this.scopePath));
      const target = this.fromHostUrl(new URL(mount, window.location.href));
      this.goto(target ?? scopeRoot, { notify: false });
    }
    for (const location of later) {
      this.gotoHostLocation(location);
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
  private addScope(path: string, scope = this.scopePath): string {
    if (!scope) return path;
    if (path === "/") return scope;
    return scope + path;
  }

  /** Href for an internal path: basePath, then the encoded scope-relative path. */
  prefixPath = (path: string): string => {
    return this.basePath + encodePath(this.stripScope(path));
  };

  /** Href for a site-absolute href, as the renderer writes them: `/guide?x#y`. */
  hostHref = (href: string): string => {
    const url = new URL(href, window.location.origin);
    return this.prefixPath(decode(url.pathname)) + url.search + url.hash;
  };

  /** Link target for a nav item or breadcrumb: its resolved cross-section `href`, else its prefixed path. */
  hrefFor = (target: { path: string; href?: string }): string => {
    return target.href ?? this.prefixPath(target.path);
  };

  goto = (newPath: string, { notify = true } = {}) => {
    // pushState resolves a relative path against the current URL; match it.
    const url = new URL(newPath, window.location.href);
    const path = decode(url.pathname);

    if (!this.embedded) {
      window.history.pushState({}, "", newPath);
    } else if (notify && this.onNavigate) {
      this.onNavigate(this.prefixPath(path) + url.search + url.hash);
    }

    this.path = path;
    this.hash = decode(url.hash.slice(1));
  };

  /** Go to an anchor on this page. Keeps `history.state`, where an embedding
   *  host's router keeps its location state. */
  gotoHash = (hash: string) => {
    window.history.pushState(window.history.state, "", `#${hash}`);
    this.hash = hash;
  };

  /** Go where the host asked: `location` is the host URL's path, query and
   *  hash, as the browser shows them. A location outside basePath is not the
   *  viewer's to show. `onNavigate` reports only navigation the viewer starts,
   *  as a controlled input's `onChange` does, so this skips it. */
  gotoHostLocation = (location: string) => {
    if (!this.resolved) {
      this.pendingGotos.push(location);
      return;
    }
    const target = this.fromHostUrl(new URL(location, window.location.href));
    if (target !== null) this.goto(target, { notify: false });
  };

  /** Map a host URL under basePath to the URL `goto` takes; null outside it. */
  private fromHostUrl({ pathname, search, hash }: URL): string | null {
    // Encodes raw non-ASCII as `pathname` does, but leaves `%` escapes as given.
    const basePath = this.basePath && new URL(this.basePath, window.location.href).pathname;
    if (basePath && pathname !== basePath && !pathname.startsWith(basePath + "/")) return null;
    const rest = pathname.slice(basePath.length) || "/";
    // `goto` parses a URL, so the scope joins `rest` encoded.
    return this.addScope(rest, encodePath(this.scopePath)) + search + hash;
  }

  /** Call once on mount. Only clicks inside `rootElement` are routed: an
   *  embedding host's own links stay its own. */
  initRouter = (rootElement: HTMLElement): (() => void) => {
    const handlePopState = () => {
      this.path = decode(window.location.pathname);
      this.hash = decode(window.location.hash.slice(1));
    };

    const handleClick = (e: MouseEvent) => {
      // A viewer handler, such as the outline's, already navigated.
      if (e.defaultPrevented) return;
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
      // The browser's own fragment navigation would null the host's history.state.
      if (this.embedded && href.startsWith("#")) {
        e.preventDefault();
        const hash = decode(href.slice(1));
        // Only a changed hash makes `PageContent` scroll to it.
        if (hash === this.hash) {
          document.getElementById(hash)?.scrollIntoView({ behavior: "auto" });
        }
        this.gotoHash(hash);
        return;
      }
      if (isExternalLink(href, anchor)) return;

      if (!this.resolved) return;

      // The renderer leaves non-`.md` relative links as written; in embedded
      // mode `baseURI` is the host page, which maps back through basePath.
      const url = new URL(href, anchor.baseURI);
      if (url.origin !== window.location.origin) return;
      const target = this.fromHostUrl(url);

      // Cross-section link — let host handle routing via onNavigate
      if (target === null) {
        if (this.onNavigate) {
          e.preventDefault();
          this.onNavigate(url.pathname + url.search + url.hash);
        }
        return;
      }

      e.preventDefault();
      this.goto(target);
    };

    const offPopState = this.embedded ? null : on(window, "popstate", handlePopState);
    const offClick = on(rootElement, "click", handleClick);

    return () => {
      offPopState?.();
      offClick();
    };
  };
}
