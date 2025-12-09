import { writable } from "svelte/store";

/** Current URL path */
export const path = writable(window.location.pathname);

/** Extract document path by removing the /docs prefix */
export function extractDocPath(urlPath: string): string {
  return urlPath.replace(/^\/docs\/?/, "");
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

    // Handle internal navigation (links are already resolved by backend)
    e.preventDefault();
    goto(href);
  });
}
