import { writable } from "svelte/store";
import type { PageResponse } from "../types";
import { fetchPage, NotFoundError } from "../api/client";

interface PageState {
  data: PageResponse | null;
  loading: boolean;
  error: string | null;
  notFound: boolean;
}

const initialState: PageState = {
  data: null,
  loading: false,
  error: null,
  notFound: false,
};

function createPageStore() {
  const { subscribe, set, update } = writable<PageState>(initialState);
  let abortController: AbortController | null = null;

  return {
    subscribe,

    /** Load a page by path, cancelling any in-flight request */
    async load(path: string, options?: { bypassCache?: boolean; silent?: boolean }) {
      // Cancel any in-flight request
      if (abortController) {
        abortController.abort();
      }
      abortController = new AbortController();

      // Silent mode skips loading state (used for live reload to preserve scroll)
      // Keep previous data during loading for smooth transitions on fast loads
      if (!options?.silent) {
        update((state) => ({ ...state, loading: true, error: null, notFound: false }));
      }

      try {
        const data = await fetchPage(path, {
          bypassCache: options?.bypassCache,
          signal: abortController.signal,
        });
        set({ data, loading: false, error: null, notFound: false });
        if (data.meta.title) {
          document.title = `${data.meta.title} - RW`;
        }
      } catch (e) {
        // Ignore aborted requests
        if (e instanceof DOMException && e.name === "AbortError") {
          return;
        }
        if (e instanceof NotFoundError) {
          set({ data: null, loading: false, error: null, notFound: true });
        } else {
          const message = e instanceof Error ? e.message : "Unknown error";
          set({ data: null, loading: false, error: message, notFound: false });
        }
      } finally {
        abortController = null;
      }
    },

    /** Clear the current page */
    clear() {
      set(initialState);
    },
  };
}

export const page = createPageStore();
