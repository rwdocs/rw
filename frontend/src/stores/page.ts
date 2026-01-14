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

  return {
    subscribe,

    /** Load a page by path */
    async load(path: string, options?: { bypassCache?: boolean }) {
      update((state) => ({
        ...state,
        loading: true,
        error: null,
        notFound: false,
      }));
      try {
        const data = await fetchPage(path, options);
        set({ data, loading: false, error: null, notFound: false });
        // Update document title
        if (data.meta.title) {
          document.title = `${data.meta.title} - Docstage`;
        }
      } catch (e) {
        if (e instanceof NotFoundError) {
          set({ data: null, loading: false, error: null, notFound: true });
        } else {
          const message = e instanceof Error ? e.message : "Unknown error";
          set({ data: null, loading: false, error: message, notFound: false });
        }
      }
    },

    /** Clear the current page */
    clear() {
      set(initialState);
    },
  };
}

export const page = createPageStore();
