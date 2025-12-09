import { writable, get } from "svelte/store";

/** Current URL path */
export const path = writable(window.location.pathname);

/** Extract document path by removing the /docs prefix */
export function extractDocPath(urlPath: string): string {
  return urlPath.replace(/^\/docs\/?/, "");
}

/**
 * Resolve a link href to an absolute SPA path.
 * Handles relative paths (./page.md, ../other.md) and strips .md extensions.
 */
export function resolveLink(href: string, currentPath: string): string {
  let resultPath: string;

  if (href.startsWith("/")) {
    // Already absolute path
    resultPath = href;
  } else {
    // Relative path - resolve against current location using URL API
    // Add trailing slash so URL treats current path as a directory, not a file.
    // Without this, "adr-101/index.md" resolved against "/docs/foo/adr"
    // would yield "/docs/foo/adr-101/index.md" instead of "/docs/foo/adr/adr-101/index.md"
    const basePath = currentPath.endsWith("/") ? currentPath : currentPath + "/";
    const base = new URL(basePath, window.location.origin);
    const resolved = new URL(href, base);
    resultPath = resolved.pathname;
  }

  // Strip .md extension and /index suffix for clean URLs
  resultPath = resultPath.replace(/\/index\.md$/, "").replace(/\.md$/, "");

  // Ensure /docs prefix for all documentation links
  if (!resultPath.startsWith("/docs")) {
    resultPath = "/docs" + resultPath;
  }

  return resultPath;
}

/** Navigate to a path programmatically */
export function goto(newPath: string) {
  window.history.pushState({}, "", newPath);
  path.set(newPath);
}

/** Initialize router - call once on app mount */
export function initRouter() {
  // Handle browser back/forward navigation
  window.addEventListener("popstate", () => {
    path.set(window.location.pathname);
  });

  // Intercept link clicks for SPA navigation
  document.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const anchor = target.closest("a");

    if (!anchor) return;

    const href = anchor.getAttribute("href");
    if (!href) return;

    // Skip non-local links
    const isExternal =
      href.startsWith("http") ||
      href.startsWith("//") ||
      href.startsWith("mailto:") ||
      href.startsWith("tel:");
    if (
      isExternal ||
      href.startsWith("#") ||
      anchor.hasAttribute("target") ||
      anchor.hasAttribute("download")
    ) {
      return;
    }

    // Handle internal navigation
    e.preventDefault();
    const currentPath = get(path);
    const resolvedPath = resolveLink(href, currentPath);
    goto(resolvedPath);
  });
}
