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
  const { subscribe, set } = writable<PageState>(initialState);
  let abortController: AbortController | null = null;

  return {
    subscribe,

    /** Load a page by path, cancelling any in-flight request */
    async load(path: string, options?: { bypassCache?: boolean }) {
      // Cancel any in-flight request
      if (abortController) {
        abortController.abort();
      }
      abortController = new AbortController();

      // Atomic reset to loading state
      set({ ...initialState, loading: true });

      try {
        const data = await fetchPage(path, { ...options, signal: abortController.signal });
        set({ data, loading: false, error: null, notFound: false });
        // Update document title
        if (data.meta.title) {
          document.title = `${data.meta.title} - Docstage`;
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
