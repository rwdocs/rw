import { writable } from "svelte/store";
import type { Readable } from "svelte/store";

/** Extract document path for API calls (strip leading slash) */
export function extractDocPath(urlPath: string): string {
  return urlPath.replace(/^\//, "");
}

export interface RouterStore {
  path: Readable<string>;
  hash: Readable<string>;
  embedded: boolean;
  goto(newPath: string): void;
  initRouter(rootElement?: HTMLElement): () => void;
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

/** Create a router store instance */
export function createRouter(options?: { embedded?: boolean; initialPath?: string }): RouterStore {
  const embedded = options?.embedded ?? false;

  /** Current URL path */
  const path = writable(
    embedded && options?.initialPath ? options.initialPath.split("#")[0] : window.location.pathname,
  );

  /** Current URL hash (without the # prefix) */
  const hash = writable(
    embedded && options?.initialPath
      ? (options.initialPath.split("#")[1] ?? "")
      : window.location.hash.slice(1),
  );

  /** Navigate to a path programmatically */
  function goto(newPath: string) {
    const origin = typeof window !== "undefined" ? window.location.origin : "http://localhost";
    const url = new URL(newPath, origin);

    if (!embedded) {
      window.history.pushState({}, "", newPath);
    }

    path.set(url.pathname);
    hash.set(url.hash.slice(1));

    // If there's a hash, scrolling will be handled by the page component
    // Otherwise scroll to top
    if (!url.hash && !embedded) {
      window.scrollTo(0, 0);
    }
  }

  /** Initialize router - call once on app mount. Returns cleanup function.
   * In embedded mode, pass the app's root element to scope click handling
   * to links within the RW app instead of the entire document. */
  function initRouter(rootElement?: HTMLElement): () => void {
    // Handle browser back/forward navigation
    const handlePopState = () => {
      path.set(window.location.pathname);
      hash.set(window.location.hash.slice(1));
    };

    // Intercept link clicks for SPA navigation
    const handleClick = (e: MouseEvent) => {
      const target = e.target;
      if (!(target instanceof Element)) return;

      const anchor = target.closest("a");
      if (!anchor) return;

      const href = anchor.getAttribute("href");
      if (!href) return;

      // Skip if modifier key pressed (allow Cmd/Ctrl+click to open in new tab)
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;

      // Skip external links, fragment links, and links with target/download
      if (isExternalLink(href, anchor)) return;

      // Handle internal navigation (links are already resolved by backend)
      e.preventDefault();
      goto(href);
    };

    // In embedded mode, scope the click handler to the root element to avoid
    // intercepting clicks in the host application
    const clickTarget: Document | HTMLElement = embedded && rootElement ? rootElement : document;

    if (!embedded) {
      window.addEventListener("popstate", handlePopState);
    }
    clickTarget.addEventListener("click", handleClick as EventListener);

    return () => {
      if (!embedded) {
        window.removeEventListener("popstate", handlePopState);
      }
      clickTarget.removeEventListener("click", handleClick as EventListener);
    };
  }

  return { path, hash, embedded, goto, initRouter };
}
