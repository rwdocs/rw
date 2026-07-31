import { SvelteSet } from "svelte/reactivity";
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
  // `SvelteSet`, not a `$state` Set: `$state` proxies plain objects and arrays
  // but not collections, so a plain Set would ignore `.add()`/`.delete()` and
  // update only on reassignment.
  collapsed = new SvelteSet<string>();
  currentSectionRef = $state<string | undefined>(undefined);

  private apiClient: ApiClient;
  private currentController: AbortController | null = null;
  private activePath: string | null = null;
  private sectionRefResolver?: SectionRefResolver;

  constructor(apiClient: ApiClient) {
    this.apiClient = apiClient;
  }

  /** True when the navigation tree has loaded but contains no items and has no
   *  parent-scope back-link — i.e. there is nothing to navigate to. */
  get isEmpty(): boolean {
    const tree = this.tree;
    return tree !== null && tree.items.length === 0 && !tree.parentScope;
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

      let resolvedTree = tree;
      if (this.sectionRefResolver) {
        try {
          resolvedTree = await resolveNavTree(tree, this.sectionRefResolver);
          if (controller.signal.aborted) return;
        } catch (e) {
          if (controller.signal.aborted) return; // superseded load: don't commit
          if (import.meta.env.DEV) {
            console.warn(
              "[rw] nav section-ref resolution failed; using unresolved navigation tree:",
              e,
            );
          }
          // keep the fetched (unresolved) tree
        }
      }

      const allParentPaths = collectParentPaths(resolvedTree.items);
      this.tree = resolvedTree;
      this.loading = false;
      this.replaceCollapsed(allParentPaths);
      this.currentSectionRef = sectionRef;

      if (this.activePath) {
        this.doExpandTo(this.activePath);
      }
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      // A superseded load (a newer loadSection() aborted this controller) must
      // not write state, even when its fetch/resolver rejects with a plain
      // Error rather than an AbortError. Mirrors the success-path guards above.
      if (controller.signal.aborted) return;
      const message = e instanceof Error ? e.message : "Unknown error";
      this.error = message;
      this.loading = false;
    }
  };

  toggle = (path: string) => {
    if (this.collapsed.has(path)) {
      this.collapsed.delete(path);
    } else {
      this.collapsed.add(path);
    }
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
    this.collapsed.clear();
    this.currentSectionRef = undefined;
  };

  private doExpandTo = (path: string): void => {
    if (!this.tree) return;

    const pathsToExpand = getParentPaths(path);
    const alreadyCorrect = pathsToExpand.every((p) => !this.collapsed.has(p));
    if (alreadyCorrect) return;

    const next = new Set(collectParentPaths(this.tree.items));
    for (const parentPath of pathsToExpand) {
      next.delete(parentPath);
    }
    this.replaceCollapsed(next);
  };

  /** Make `collapsed` hold exactly `paths`, touching only the keys that differ.
   *
   *  Clearing and re-adding would work, but every delete and add bumps the
   *  set's version, and `SvelteSet.has()` falls back to tracking that version
   *  for keys it does not hold — so a wholesale rebuild wakes every reader
   *  whose path is currently expanded. `SvelteSet.add()` already skips keys it
   *  has, so only the removals need a guard here. */
  private replaceCollapsed = (paths: Iterable<string>) => {
    const next = new Set<string>(paths);
    // Snapshot before mutating: deleting while iterating the live set is a
    // ConcurrentModification hazard in the general case.
    for (const path of [...this.collapsed]) {
      if (!next.has(path)) this.collapsed.delete(path);
    }
    for (const path of next) this.collapsed.add(path);
  };
}
