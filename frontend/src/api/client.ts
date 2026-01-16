import type { ConfigResponse, NavigationTree, PageResponse } from "../types";

const API_BASE = "/api";

/** Options for API fetch functions */
interface FetchOptions {
  bypassCache?: boolean;
}

/** Build RequestInit from fetch options */
function buildRequestInit(options?: FetchOptions): RequestInit {
  return options?.bypassCache ? { cache: "no-store" } : {};
}

/** Error thrown when a page is not found */
export class NotFoundError extends Error {
  constructor(public path: string) {
    super(`Page not found: ${path}`);
    this.name = "NotFoundError";
  }
}

/** Fetch the navigation tree */
export async function fetchNavigation(options?: FetchOptions): Promise<NavigationTree> {
  const response = await fetch(`${API_BASE}/navigation`, buildRequestInit(options));
  if (!response.ok) {
    throw new Error(`Failed to fetch navigation: ${response.status} ${response.statusText}`);
  }
  return response.json();
}

/** Fetch a page by path */
export async function fetchPage(path: string, options?: FetchOptions): Promise<PageResponse> {
  const response = await fetch(`${API_BASE}/pages/${path}`, buildRequestInit(options));
  if (!response.ok) {
    if (response.status === 404) {
      throw new NotFoundError(path);
    }
    throw new Error(`Failed to fetch page: ${response.status} ${response.statusText}`);
  }
  return response.json();
}

/** Fetch server config */
export async function fetchConfig(): Promise<ConfigResponse> {
  const response = await fetch(`${API_BASE}/config`);
  if (!response.ok) {
    throw new Error(`Failed to fetch config: ${response.status} ${response.statusText}`);
  }
  return response.json();
}
