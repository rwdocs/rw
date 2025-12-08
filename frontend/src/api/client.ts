import type { NavigationTree, PageResponse } from "../types";

const API_BASE = "/api";

/** Error thrown when a page is not found */
export class NotFoundError extends Error {
  constructor(public path: string) {
    super(`Page not found: ${path}`);
    this.name = "NotFoundError";
  }
}

/** Fetch the navigation tree */
export async function fetchNavigation(options?: {
  bypassCache?: boolean;
}): Promise<NavigationTree> {
  const fetchOptions: RequestInit = options?.bypassCache
    ? { cache: "no-store" }
    : {};
  const response = await fetch(`${API_BASE}/navigation`, fetchOptions);
  if (!response.ok) {
    throw new Error(
      `Failed to fetch navigation: ${response.status} ${response.statusText}`,
    );
  }
  return response.json();
}

/** Fetch a page by path */
export async function fetchPage(
  path: string,
  options?: { bypassCache?: boolean },
): Promise<PageResponse> {
  const fetchOptions: RequestInit = options?.bypassCache
    ? { cache: "no-store" }
    : {};
  const response = await fetch(`${API_BASE}/pages/${path}`, fetchOptions);
  if (!response.ok) {
    if (response.status === 404) {
      throw new NotFoundError(path);
    }
    throw new Error(
      `Failed to fetch page: ${response.status} ${response.statusText}`,
    );
  }
  return response.json();
}
