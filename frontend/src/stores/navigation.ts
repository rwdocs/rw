import { writable } from "svelte/store";
import type { NavigationTree, NavItem } from "../types";
import { fetchNavigation } from "../api/client";

export interface NavigationState {
  tree: NavigationTree | null;
  loading: boolean;
  error: string | null;
  collapsed: Set<string>;
  /** Current scope path (without leading slash, empty for root). */
  currentScope: string;
}

const initialState: NavigationState = {
  tree: null,
  loading: true,
  error: null,
  collapsed: new Set(),
  currentScope: "",
};

/** Collect all paths with children from the navigation tree */
export function collectParentPaths(items: NavItem[]): string[] {
  const paths: string[] = [];
  for (const item of items) {
    if (item.children && item.children.length > 0) {
      paths.push(item.path);
      paths.push(...collectParentPaths(item.children));
    }
  }
  return paths;
}

/** Get parent paths for a given path */
export function getParentPaths(path: string): string[] {
  const parts = path.split("/").filter(Boolean);
  const paths: string[] = [];
  let current = "";
  for (const part of parts) {
    current += "/" + part;
    paths.push(current);
  }
  return paths;
}

function createNavigationStore() {
  const { subscribe, set, update } = writable<NavigationState>(initialState);

  return {
    subscribe,

    /** Load navigation tree from API for the root scope */
    async load(options?: { bypassCache?: boolean }): Promise<void> {
      return this.loadScope("", options);
    },

    /** Load navigation tree for a specific scope */
    async loadScope(scope: string, options?: { bypassCache?: boolean }): Promise<void> {
      update((state) => ({ ...state, loading: true, error: null }));
      try {
        const tree = await fetchNavigation({ ...options, scope: scope || undefined });
        // Collapse all parent items by default
        const allParentPaths = collectParentPaths(tree.items);
        update((state) => ({
          ...state,
          tree,
          loading: false,
          collapsed: new Set(allParentPaths),
          currentScope: scope,
        }));
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
        return { ...state, collapsed };
      });
    },

    /** Expand only the path to the current page, collapse all others */
    expandOnlyTo(path: string) {
      update((state) => {
        if (!state.tree) return state;

        const pathsToExpand = getParentPaths(path);
        // Optimization: skip update if target path is already expanded.
        // This handles clicks on the current page or its children.
        // For navigation to different branches, the full re-collapse happens below.
        const alreadyCorrect = pathsToExpand.every((p) => !state.collapsed.has(p));
        if (alreadyCorrect) return state;

        // Collapse all parent items
        const collapsed = new Set(collectParentPaths(state.tree.items));
        // Expand the path to the current page
        for (const parentPath of pathsToExpand) {
          collapsed.delete(parentPath);
        }
        return { ...state, collapsed };
      });
    },

    /** Reset store to initial state (for testing) */
    _reset() {
      set({ ...initialState, collapsed: new Set() });
    },
  };
}

export const navigation = createNavigationStore();
