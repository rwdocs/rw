import { SvelteSet } from "svelte/reactivity";
import { untrack } from "svelte";
import type { NavigationTree, NavItem, SectionInfo } from "../types";
import type { ApiClient } from "../api/client";
import { SectionLinks, sectionRefString } from "$lib/sectionRefs";

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
  /** Ref of the section the shown tree is scoped to, as the response names it. */
  currentSectionRef: string | undefined = undefined;

  private apiClient: ApiClient;
  private currentController: AbortController | null = null;
  private inFlight: { sectionRef: string | undefined; done: Promise<void> } | null = null;
  private activePath: string | null = null;
  private readonly links: SectionLinks;

  constructor(apiClient: ApiClient, links = new SectionLinks()) {
    this.apiClient = apiClient;
    this.links = links;
  }

  /** The host's URL for a section root, when the host maps the section. */
  sectionHref = (item: { section?: SectionInfo }): string | undefined =>
    item.section &&
    this.links.href(sectionRefString(item.section), "", this.tree?.sectionAncestry ?? {});

  get isEmpty(): boolean {
    const tree = this.tree;
    return tree !== null && tree.items.length === 0 && !tree.parentScope;
  }

  load = (sectionRef?: string, options?: { bypassCache?: boolean }): Promise<void> => {
    const entry = { sectionRef, done: this.fetchTree(sectionRef, options) };
    this.inFlight = entry;
    void entry.done.finally(() => {
      if (this.inFlight === entry) this.inFlight = null;
    });
    return entry.done;
  };

  /** Load `sectionRef`'s tree unless it is already shown or on its way. */
  loadSection = async (sectionRef: string): Promise<void> => {
    const pending = this.inFlight;
    if (pending?.sectionRef === sectionRef) return pending.done;
    // An unscoped load learns its scope only from the response, which may
    // name this very section.
    if (pending && pending.sectionRef === undefined) {
      await pending.done;
      return this.loadSection(sectionRef);
    }
    // A load for another section would replace the shown tree when it lands.
    if (pending || sectionRef !== this.currentSectionRef) return this.load(sectionRef);
  };

  private fetchTree = async (
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
      await this.links.ensure(tree.sectionAncestry);
      if (controller.signal.aborted) return;

      const shownRef = sectionRefString(tree.scope.section);
      const known = new Set(
        this.tree && shownRef === this.currentSectionRef ? collectParentPaths(this.tree.items) : [],
      );
      const activeParents = new Set(this.activePath ? getParentPaths(this.activePath) : []);
      const collapsed = collectParentPaths(tree.items).filter((path) =>
        known.has(path) ? this.collapsed.has(path) : !activeParents.has(path),
      );
      this.tree = tree;
      this.loading = false;
      this.replaceCollapsed(collapsed);
      this.currentSectionRef = shownRef;
    } catch (e) {
      // Judge supersession by our signal, not the error: a superseded fetch can
      // reject with a plain Error, and a host fetchFn can raise its own AbortError.
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
    for (const path of this.collapsed) {
      if (!next.has(path)) this.collapsed.delete(path);
    }
    for (const path of next) this.collapsed.add(path);
  };
}
