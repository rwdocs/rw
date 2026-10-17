import type { ConfigResponse, NavigationTree, PageResponse } from "../types";
import { readJson } from "./json";

/** Options for API fetch functions */
interface FetchOptions {
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
  constructor(path: string) {
    super(`Page not found: ${path}`);
    this.name = "NotFoundError";
  }
}

/** Options for fetching navigation */
interface FetchNavigationOptions extends FetchOptions {
  /** Section ref string (e.g., "domain:default/billing") to load navigation for a specific section. */
  sectionRef?: string;
}

export interface ApiClient {
  fetchConfig(): Promise<ConfigResponse>;
  fetchPage(path: string, options?: FetchOptions): Promise<PageResponse>;
  fetchNavigation(options?: FetchNavigationOptions): Promise<NavigationTree>;
}

/** Create an API client bound to the given base URL */
export function createApiClient(apiBase: string, fetchFn: typeof fetch = fetch): ApiClient {
  const base = apiBase.replace(/\/+$/, "");

  return {
    async fetchNavigation(options?: FetchNavigationOptions): Promise<NavigationTree> {
      const params = new URLSearchParams();
      if (options?.sectionRef) {
        params.set("sectionRef", options.sectionRef);
      }
      const url = params.toString() ? `${base}/navigation?${params}` : `${base}/navigation`;
      const response = await fetchFn(url, buildRequestInit(options));
      return readJson<NavigationTree>(response, "Failed to fetch navigation");
    },

    async fetchPage(path: string, options?: FetchOptions): Promise<PageResponse> {
      const response = await fetchFn(`${base}/pages/${path}`, buildRequestInit(options));
      if (response.status === 404) {
        throw new NotFoundError(path);
      }
      return readJson<PageResponse>(response, "Failed to fetch page");
    },

    async fetchConfig(): Promise<ConfigResponse> {
      const response = await fetchFn(`${base}/config`);
      return readJson<ConfigResponse>(response, "Failed to fetch config");
    },
  };
}
