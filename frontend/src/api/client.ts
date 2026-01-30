import type { ConfigResponse, NavigationTree, PageResponse } from "../types";

const API_BASE = "/api";

/** Options for API fetch functions */
export interface FetchOptions {
  bypassCache?: boolean;
  signal?: AbortSignal;
}

/** Build RequestInit from fetch options */
function buildRequestInit(options?: FetchOptions): RequestInit {
  const init: RequestInit = {};
  if (options?.bypassCache) {
    init.cache = "no-store";
  }
  if (options?.signal) {
    init.signal = options.signal;
  }
  return init;
}

/** Error thrown when a page is not found */
export class NotFoundError extends Error {
  constructor(public path: string) {
    super(`Page not found: ${path}`);
    this.name = "NotFoundError";
  }
}

/** Options for fetching navigation */
export interface FetchNavigationOptions extends FetchOptions {
  /** Scope path (without leading slash) to load navigation for a specific section. */
  scope?: string;
}

/** Fetch the navigation tree */
export async function fetchNavigation(options?: FetchNavigationOptions): Promise<NavigationTree> {
  const params = new URLSearchParams();
  if (options?.scope) {
    params.set("scope", options.scope);
  }
  const url = params.toString() ? `${API_BASE}/navigation?${params}` : `${API_BASE}/navigation`;

  const response = await fetch(url, buildRequestInit(options));
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
