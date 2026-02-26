import { writable } from "svelte/store";
import type { Readable } from "svelte/store";

/** Extract document path for API calls (strip leading slash) */
export function extractDocPath(urlPath: string): string {
  return urlPath.replace(/^\//, "");
}

export interface RouterStore {
  path: Readable<string>;
  hash: Readable<string>;
  goto(newPath: string): void;
  initRouter(): () => void;
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
export function createRouter(options?: { embedded?: boolean }): RouterStore {
  const embedded = options?.embedded ?? false;

  /** Current URL path */
  const path = writable(window.location.pathname);

  /** Current URL hash (without the # prefix) */
  const hash = writable(window.location.hash.slice(1));

  /** Navigate to a path programmatically */
  function goto(newPath: string) {
    const url = new URL(newPath, window.location.origin);

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

  /** Initialize router - call once on app mount. Returns cleanup function. */
  function initRouter(): () => void {
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

    if (!embedded) {
      window.addEventListener("popstate", handlePopState);
    }
    document.addEventListener("click", handleClick);

    return () => {
      if (!embedded) {
        window.removeEventListener("popstate", handlePopState);
      }
      document.removeEventListener("click", handleClick);
    };
  }

  return { path, hash, goto, initRouter };
}
