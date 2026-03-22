import type { NavigationTree, NavItem } from "../types";
import type { ApiClient } from "../api/client";

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

export class Navigation {
  tree = $state.raw<NavigationTree | null>(null);
  loading = $state(true);
  error = $state<string | null>(null);
  // Reassign the entire Set to trigger reactivity — in-place .add()/.delete() won't.
  collapsed = $state<Set<string>>(new Set());
  currentSectionRef = $state<string | undefined>(undefined);

  private apiClient: ApiClient;
  private currentController: AbortController | null = null;
  private activePath: string | null = null;

  constructor(apiClient: ApiClient) {
    this.apiClient = apiClient;
  }

  load = async (options?: { bypassCache?: boolean }): Promise<void> => {
    return this.loadSection(undefined, options);
  };

  loadSection = async (
    sectionRef: string | undefined,
    options?: { bypassCache?: boolean },
  ): Promise<void> => {
    this.currentController?.abort();
    const controller = new AbortController();
    this.currentController = controller;

    this.loading = true;
    this.error = null;

    try {
      const tree = await this.apiClient.fetchNavigation({
        ...options,
        sectionRef,
        signal: controller.signal,
      });
      if (controller.signal.aborted) return;

      const allParentPaths = collectParentPaths(tree.items);
      this.tree = tree;
      this.loading = false;
      this.collapsed = new Set(allParentPaths);
      this.currentSectionRef = sectionRef;

      if (this.activePath) {
        this.doExpandTo(this.activePath);
      }
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      const message = e instanceof Error ? e.message : "Unknown error";
      this.error = message;
      this.loading = false;
    }
  };

  toggle = (path: string) => {
    const collapsed = new Set(this.collapsed);
    if (collapsed.has(path)) {
      collapsed.delete(path);
    } else {
      collapsed.add(path);
    }
    this.collapsed = collapsed;
  };

  expandOnlyTo = (path: string) => {
    this.activePath = path;
    this.doExpandTo(path);
  };

  _reset = () => {
    this.activePath = null;
    this.tree = null;
    this.loading = true;
    this.error = null;
    this.collapsed = new Set();
    this.currentSectionRef = undefined;
  };

  private doExpandTo = (path: string): void => {
    if (!this.tree) return;

    const pathsToExpand = getParentPaths(path);
    const alreadyCorrect = pathsToExpand.every((p) => !this.collapsed.has(p));
    if (alreadyCorrect) return;

    const collapsed = new Set(collectParentPaths(this.tree.items));
    for (const parentPath of pathsToExpand) {
      collapsed.delete(parentPath);
    }
    this.collapsed = collapsed;
  };
}
