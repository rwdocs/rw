import type { SectionInfo, ScopeInfo, NavItem, Breadcrumb, NavigationTree } from "../types";

export type SectionRefResolver = (refs: string[]) => Promise<Record<string, string>>;

// Allowlist matching the Angular/DOMPurify pattern: known safe schemes, or
// anything that isn't obviously a scheme (relative paths, fragments, queries).
const SAFE_HREF = /^(?:(?:https?|mailto|tel):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i;

function safeHref(candidate: string): string {
  return SAFE_HREF.test(candidate) ? candidate : "#";
}

/**
 * Build a ref string from section info (e.g., "domain:default/billing").
 * Namespace is always "default" — the backend does not expose namespace yet.
 */
export function sectionRefString(section: SectionInfo): string {
  return `${section.kind}:default/${section.name}`;
}

/**
 * Rewrite href attributes on links with data-section-ref in rendered HTML content.
 *
 * Collects unique refs, calls the resolver once, then rewrites each link's
 * href to the resolved base URL + section path.
 */
export async function rewriteSectionRefLinks(
  container: HTMLElement,
  resolver: SectionRefResolver,
  getBasePath: () => string,
): Promise<void> {
  const links = container.querySelectorAll<HTMLElement>("a[data-section-ref]");
  if (links.length === 0) return;

  const refsSet = new Set<string>();
  for (const link of links) {
    refsSet.add(link.getAttribute("data-section-ref")!);
  }

  const resolved = await resolver([...refsSet]);

  for (const link of links) {
    const ref = link.getAttribute("data-section-ref")!;
    const sectionPath = link.getAttribute("data-section-path") ?? "";
    const baseUrl = resolved[ref] ?? getBasePath();
    const href = sectionPath ? `${baseUrl}/${sectionPath}` : baseUrl;
    link.setAttribute("href", safeHref(href));
  }
}

/** Collect all unique section ref strings from a navigation tree. */
function collectNavRefs(items: NavItem[]): Set<string> {
  const refs = new Set<string>();
  for (const item of items) {
    if (item.section) {
      refs.add(sectionRefString(item.section));
    }
    if (item.children) {
      for (const ref of collectNavRefs(item.children)) {
        refs.add(ref);
      }
    }
  }
  return refs;
}

/** Rewrite paths in a navigation tree using resolved section refs. */
function rewriteNavItems(items: NavItem[], resolved: Record<string, string>): NavItem[] {
  return items.map((item) => {
    const ref = item.section ? sectionRefString(item.section) : undefined;
    const href = ref ? resolved[ref] : undefined;
    return {
      ...item,
      href,
      children: item.children ? rewriteNavItems(item.children, resolved) : undefined,
    };
  });
}

/**
 * Resolve section refs in a navigation tree and rewrite all paths.
 * Returns a new tree with rewritten paths.
 */
export async function resolveNavTree(
  tree: NavigationTree,
  resolver: SectionRefResolver,
): Promise<NavigationTree> {
  // The backend provides parentScope for all non-root sections. This fallback
  // handles older backends that may omit it for top-level sections. The root
  // scope itself (path "/") has no parent to navigate back to.
  const isRootScope = tree.scope?.path === "/";
  const effectiveParentScope: ScopeInfo | undefined =
    tree.parentScope ??
    (tree.scope && !isRootScope
      ? { path: "/", title: "Home", section: { kind: "section", name: "root" } }
      : undefined);

  const refs = collectNavRefs(tree.items);
  const scopeRef = tree.scope ? sectionRefString(tree.scope.section) : undefined;
  const parentScopeRef = effectiveParentScope
    ? sectionRefString(effectiveParentScope.section)
    : undefined;
  if (scopeRef) refs.add(scopeRef);
  if (parentScopeRef) refs.add(parentScopeRef);
  if (refs.size === 0) return tree;

  const resolved = await resolver([...refs]);

  return {
    items: rewriteNavItems(tree.items, resolved),
    scope: tree.scope
      ? {
          ...tree.scope,
          href: resolved[scopeRef!],
        }
      : undefined,
    parentScope: effectiveParentScope
      ? {
          ...effectiveParentScope,
          href: resolved[parentScopeRef!],
        }
      : undefined,
  };
}

/**
 * Resolve section refs in breadcrumbs and rewrite paths.
 * Returns new breadcrumb array with rewritten paths.
 */
export async function resolveBreadcrumbs(
  breadcrumbs: Breadcrumb[],
  resolver: SectionRefResolver,
): Promise<Breadcrumb[]> {
  const refs = new Set<string>();
  for (const bc of breadcrumbs) {
    if (bc.section) refs.add(sectionRefString(bc.section));
  }
  if (refs.size === 0) return breadcrumbs;

  const resolved = await resolver([...refs]);

  return breadcrumbs.map((bc) => {
    if (!bc.section) return bc;
    const ref = sectionRefString(bc.section);
    return { ...bc, href: resolved[ref] };
  });
}
