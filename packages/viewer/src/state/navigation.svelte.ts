import { untrack } from "svelte";
import type { NavigationTree, NavItem } from "../types";
import type { ApiClient } from "../api/client";
import type { SectionRefResolver } from "$lib/sectionRefs";
import { resolveNavTree } from "$lib/sectionRefs";

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
  private sectionRefResolver?: SectionRefResolver;

  constructor(apiClient: ApiClient) {
    this.apiClient = apiClient;
  }

  /** Configure section ref resolution for nav item path rewriting. */
  setSectionRefResolver(resolver: SectionRefResolver) {
    this.sectionRefResolver = resolver;
  }

  load = async (options?: { bypassCache?: boolean; sectionRef?: string }): Promise<void> => {
    return this.loadSection(options?.sectionRef, options);
  };

  loadSection = async (
    sectionRef: string | undefined,
    options?: { bypassCache?: boolean },
  ): Promise<void> => {
    this.currentController?.abort();
    const controller = new AbortController();
    this.currentController = controller;

    // Only show loading state on initial load — during live reload, keep
    // displaying the existing tree while fetching updated data in the background.
    // Use untrack to avoid creating a reactive dependency on `tree` — this method
    // is called from an $effect chain, and tracking `tree` would cause an infinite loop.
    if (!untrack(() => this.tree)) {
      this.loading = true;
    }
    this.error = null;

    try {
      const tree = await this.apiClient.fetchNavigation({
        ...options,
        sectionRef,
        signal: controller.signal,
      });
      if (controller.signal.aborted) return;

      const resolvedTree = this.sectionRefResolver
        ? await resolveNavTree(tree, this.sectionRefResolver)
        : tree;
      if (controller.signal.aborted) return;

      const allParentPaths = collectParentPaths(resolvedTree.items);
      this.tree = resolvedTree;
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
