import type { ConfigResponse, NavigationTree, PageResponse } from "../types";

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

export interface ApiClient {
  fetchConfig(): Promise<ConfigResponse>;
  fetchPage(path: string, options?: FetchOptions): Promise<PageResponse>;
  fetchNavigation(options?: FetchNavigationOptions): Promise<NavigationTree>;
}

/** Create an API client bound to the given base URL */
export function createApiClient(apiBase: string = "/api", fetchFn?: typeof fetch): ApiClient {
  const doFetch = fetchFn ?? fetch;
  const base = apiBase.replace(/\/+$/, "");

  return {
    async fetchNavigation(options?: FetchNavigationOptions): Promise<NavigationTree> {
      const params = new URLSearchParams();
      if (options?.scope) {
        params.set("scope", options.scope);
      }
      const url = params.toString() ? `${base}/navigation?${params}` : `${base}/navigation`;

      const response = await doFetch(url, buildRequestInit(options));
      if (!response.ok) {
        throw new Error(`Failed to fetch navigation: ${response.status} ${response.statusText}`);
      }
      return response.json();
    },

    async fetchPage(path: string, options?: FetchOptions): Promise<PageResponse> {
      const response = await doFetch(`${base}/pages/${path}`, buildRequestInit(options));
      if (!response.ok) {
        if (response.status === 404) {
          throw new NotFoundError(path);
        }
        throw new Error(`Failed to fetch page: ${response.status} ${response.statusText}`);
      }
      return response.json();
    },

    async fetchConfig(): Promise<ConfigResponse> {
      const response = await doFetch(`${base}/config`);
      if (!response.ok) {
        throw new Error(`Failed to fetch config: ${response.status} ${response.statusText}`);
      }
      return response.json();
    },
  };
}
