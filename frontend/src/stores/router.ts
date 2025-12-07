import { writable } from "svelte/store";

/** Current URL path */
export const path = writable(window.location.pathname);

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

    // Skip external links, hash links, and links with target
    if (
      href.startsWith("http") ||
      href.startsWith("#") ||
      anchor.hasAttribute("target") ||
      anchor.hasAttribute("download")
    ) {
      return;
    }

    // Handle internal navigation
    e.preventDefault();
    goto(href);
  });
}
