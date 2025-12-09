import { writable } from "svelte/store";
import type { NavigationTree, NavItem } from "../types";
import { fetchNavigation } from "../api/client";

interface NavigationState {
  tree: NavigationTree | null;
  loading: boolean;
  error: string | null;
  collapsed: Set<string>;
}

/** Collect all paths with children from the navigation tree */
function collectParentPaths(items: NavItem[]): string[] {
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
function getParentPaths(path: string): string[] {
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
  const { subscribe, update } = writable<NavigationState>({
    tree: null,
    loading: true,
    error: null,
    collapsed: new Set(),
  });

  return {
    subscribe,

    /** Load navigation tree from API */
    async load(options?: { bypassCache?: boolean }) {
      update((state) => ({ ...state, loading: true, error: null }));
      try {
        const tree = await fetchNavigation(options);
        // Collapse all parent items by default
        const allParentPaths = collectParentPaths(tree.items);
        update((state) => ({
          ...state,
          tree,
          loading: false,
          collapsed: new Set(allParentPaths),
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
        // Collapse all parent items
        const collapsed = new Set(collectParentPaths(state.tree.items));
        // Expand the path to the current page
        for (const parentPath of getParentPaths(path)) {
          collapsed.delete(parentPath);
        }
        return { ...state, collapsed };
      });
    },
  };
}

export const navigation = createNavigationStore();
