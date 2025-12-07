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
export async function fetchNavigation(): Promise<NavigationTree> {
  const response = await fetch(`${API_BASE}/navigation`);
  if (!response.ok) {
    throw new Error("Failed to fetch navigation");
  }
  return response.json();
}

/** Fetch a page by path */
export async function fetchPage(path: string): Promise<PageResponse> {
  const response = await fetch(`${API_BASE}/pages/${path}`);
  if (!response.ok) {
    if (response.status === 404) {
      throw new NotFoundError(path);
    }
    throw new Error("Failed to fetch page");
  }
  return response.json();
}
