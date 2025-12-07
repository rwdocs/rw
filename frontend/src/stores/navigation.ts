import { writable } from "svelte/store";
import type { NavigationTree } from "../types";
import { fetchNavigation } from "../api/client";

const COLLAPSED_STORAGE_KEY = "docstage-nav-collapsed";

interface NavigationState {
  tree: NavigationTree | null;
  loading: boolean;
  error: string | null;
  collapsed: Set<string>;
}

function loadCollapsedFromStorage(): Set<string> {
  try {
    const stored = localStorage.getItem(COLLAPSED_STORAGE_KEY);
    if (stored) {
      return new Set(JSON.parse(stored));
    }
  } catch {
    // Ignore storage errors
  }
  return new Set();
}

function saveCollapsedToStorage(collapsed: Set<string>) {
  try {
    localStorage.setItem(COLLAPSED_STORAGE_KEY, JSON.stringify([...collapsed]));
  } catch {
    // Ignore storage errors
  }
}

function createNavigationStore() {
  const { subscribe, update } = writable<NavigationState>({
    tree: null,
    loading: true,
    error: null,
    collapsed: loadCollapsedFromStorage(),
  });

  return {
    subscribe,

    /** Load navigation tree from API */
    async load() {
      update((state) => ({ ...state, loading: true, error: null }));
      try {
        const tree = await fetchNavigation();
        update((state) => ({ ...state, tree, loading: false }));
      } catch (e) {
        const message = e instanceof Error ? e.message : "Unknown error";
        update((state) => ({ ...state, error: message, loading: false }));
      }
    },

    /** Toggle collapsed state of a navigation item */
    toggle(path: string) {
      update((state) => {
        const collapsed = new Set(state.collapsed);
        if (collapsed.has(path)) {
          collapsed.delete(path);
        } else {
          collapsed.add(path);
        }
        saveCollapsedToStorage(collapsed);
        return { ...state, collapsed };
      });
    },

    /** Expand all parents of a path */
    expandTo(path: string) {
      update((state) => {
        const collapsed = new Set(state.collapsed);
        const parts = path.split("/").filter(Boolean);
        let current = "";
        for (const part of parts) {
          current += "/" + part;
          collapsed.delete(current);
        }
        saveCollapsedToStorage(collapsed);
        return { ...state, collapsed };
      });
    },
  };
}

export const navigation = createNavigationStore();
