import type {
  SectionInfo,
  SectionAncestry,
  ScopeInfo,
  NavItem,
  Breadcrumb,
  NavigationTree,
} from "../types";

/**
 * Host-supplied resolver mapping section refs to base URLs (see `mountRw`'s
 * `resolveSectionRefs` option). Contract: the host MUST map the site-root
 * section ref (e.g. `section:default/root`) to a base URL — it is the
 * guaranteed backstop every link ultimately resolves against, since every
 * ref's ancestry chain ends at the root. A host that omits it is
 * misconfigured; see the local-routing fallback in `rewriteSectionRefLinks`
 * for what happens then.
 */
export type SectionRefResolver = (refs: string[]) => Promise<Record<string, string>>;

/**
 * Build a ref string from section info (e.g., "domain:payments/billing").
 */
export function sectionRefString(section: SectionInfo): string {
  return `${section.kind}:${section.namespace}/${section.name}`;
}

/** Join a section-relative ancestor subpath with a link's own subpath. */
function joinSubpath(base: string, sub: string): string {
  return base && sub ? `${base}/${sub}` : base || sub;
}

/**
 * Join a resolved host base with a subpath, stripping a single trailing slash
 * from `base` first so a base that already ends in "/" doesn't produce a
 * doubled slash. Returns `base` unchanged when `rest` is empty.
 */
function joinBase(base: string, rest: string): string {
  return rest ? `${base.replace(/\/$/, "")}/${rest}` : base;
}

/**
 * Resolve a link's nearest `(sectionRef, subpath)` to a host URL by walking the
 * ref's ancestry chain (from the response's `sectionAncestry` map) to the
 * nearest host-mapped ancestor, then joining that ancestor's subpath with the
 * link's own. Every chain ends at the site-root ref, which the host is
 * required to map (see `SectionRefResolver`), so this only returns `undefined`
 * when the ref itself is absent from the ancestry map or the host violates
 * that contract — the caller then falls back to local routing.
 */
export function ancestryHref(
  sectionRef: string | undefined,
  subpath: string,
  ancestry: SectionAncestry | undefined,
  resolved: Record<string, string>,
): string | undefined {
  const chain = sectionRef ? ancestry?.[sectionRef] : undefined;
  if (!chain) return undefined;
  for (const a of chain) {
    const base = resolved[a.sectionRef];
    // A JS host isn't type-checked against Record<string, string> at runtime
    // and may return null (or "") for a ref it declines to map; treat only a
    // non-empty string as mapped so those fall through to the next ancestor.
    if (typeof base === "string" && base !== "") {
      const rest = joinSubpath(a.subpath, subpath);
      return joinBase(base, rest);
    }
  }
  return undefined;
}

/** The refs needed to resolve `presentRefs`: each present ref plus every ancestor
 *  in its chain (so an unmapped nearest ref can fall through to a mapped ancestor). */
function chainRefsFor(
  presentRefs: Iterable<string>,
  ancestry: SectionAncestry | undefined,
): Set<string> {
  const refs = new Set<string>();
  if (!ancestry) return refs;
  for (const ref of presentRefs) {
    const chain = ancestry[ref];
    if (chain) for (const a of chain) refs.add(a.sectionRef);
  }
  return refs;
}

/**
 * Rewrite href attributes on internal links in rendered HTML content.
 *
 * Each link carries a flat `data-section-ref` (its nearest section) and an
 * optional `data-section-path` (the target relative to that section). The
 * ordered ancestry for every section arrives once per page in `ancestry`; this
 * resolves only the refs needed for the links actually present on the page
 * (each present ref's own ancestry chain) with the host in one call, then
 * rewrites each link via its ancestry chain.
 *
 * `signal`, when provided, is checked after the resolver settles: if already
 * aborted, the function returns without touching the DOM. This guards against
 * an older in-flight call (e.g. a superseded live-reload) overwriting hrefs a
 * newer call already resolved — see the caller in `PageContent.svelte`.
 */
export async function rewriteSectionRefLinks(
  container: HTMLElement,
  resolver: SectionRefResolver,
  getBasePath: () => string,
  ancestry: SectionAncestry | undefined,
  signal?: AbortSignal,
): Promise<void> {
  const links = container.querySelectorAll<HTMLElement>("a[data-section-ref]");
  if (links.length === 0) return;

  const present = new Set<string>();
  for (const link of links) {
    const sectionRef = link.getAttribute("data-section-ref");
    if (sectionRef) present.add(sectionRef);
  }

  const refs = chainRefsFor(present, ancestry);
  if (refs.size === 0) return;

  const resolved = await resolver([...refs]);
  if (signal?.aborted) return;

  for (const link of links) {
    const sectionRef = link.getAttribute("data-section-ref") ?? undefined;
    const subpath = link.getAttribute("data-section-path") ?? "";
    // The fragment is stripped from data-section-path by the backend
    // (Sections::find drops "#…"), so it must be read back off the link's
    // current href and re-appended to whichever href we compute below. A query
    // string is NOT stripped by the backend — it stays inside data-section-path
    // (and thus `subpath`), so it is preserved without re-appending here.
    const cur = link.getAttribute("href") ?? "";
    const hashIdx = cur.indexOf("#");
    const hash = hashIdx >= 0 ? cur.slice(hashIdx) : "";

    let href = ancestryHref(sectionRef, subpath, ancestry, resolved);
    if (href === undefined) {
      // Last resort: no ancestor in the chain — including the site-root ref,
      // which the host is required to map (see `SectionRefResolver`) — is
      // host-mapped. This only happens when the host violates that contract.
      // Fall back to local routing, joined with the link's own subpath so
      // distinct unmapped links don't all collapse onto the bare mount root.
      const base = getBasePath();
      href = joinBase(base, subpath);
    }
    href += hash;
    link.setAttribute("href", href);
  }
}

/** Collect every nav item's section ref, recursing into children. */
function collectNavItemRefs(items: NavItem[], refs: Set<string>): void {
  for (const item of items) {
    if (item.section) refs.add(sectionRefString(item.section));
    if (item.children) collectNavItemRefs(item.children, refs);
  }
}

/** Rewrite paths in a navigation tree using resolved section refs. */
function rewriteNavItems(
  items: NavItem[],
  ancestry: SectionAncestry | undefined,
  resolved: Record<string, string>,
): NavItem[] {
  return items.map((item) => {
    // Section-root items resolve by their own ref (empty subpath); walking the
    // ancestry chain lets an unmapped section fall through to a mapped parent.
    const href = item.section
      ? ancestryHref(sectionRefString(item.section), "", ancestry, resolved)
      : undefined;
    return {
      ...item,
      href,
      children: item.children ? rewriteNavItems(item.children, ancestry, resolved) : undefined,
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
      ? {
          path: "/",
          title: "Home",
          // Inherit namespace from the current scope so the synthesized root
          // ref matches a custom-namespace site; fall back to "default" when
          // an older backend omits the field entirely (runtime would otherwise
          // produce "section:undefined/root").
          section: {
            kind: "section",
            namespace: tree.scope!.section.namespace ?? "default",
            name: "root",
          },
        }
      : undefined);

  const ancestry = tree.sectionAncestry;

  const present = new Set<string>();
  collectNavItemRefs(tree.items, present);
  if (tree.scope) present.add(sectionRefString(tree.scope.section));
  if (effectiveParentScope) present.add(sectionRefString(effectiveParentScope.section));

  const refs = chainRefsFor(present, ancestry);
  if (refs.size === 0) return tree;

  const resolved = await resolver([...refs]);

  return {
    ...tree,
    items: rewriteNavItems(tree.items, ancestry, resolved),
    scope: tree.scope
      ? {
          ...tree.scope,
          href: ancestryHref(sectionRefString(tree.scope.section), "", ancestry, resolved),
        }
      : undefined,
    parentScope: effectiveParentScope
      ? {
          ...effectiveParentScope,
          href: ancestryHref(
            sectionRefString(effectiveParentScope.section),
            "",
            ancestry,
            resolved,
          ),
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
  ancestry: SectionAncestry | undefined,
  resolver: SectionRefResolver,
): Promise<Breadcrumb[]> {
  const present = new Set<string>();
  for (const bc of breadcrumbs) {
    if (bc.sectionRef) present.add(bc.sectionRef);
  }

  const refs = chainRefsFor(present, ancestry);
  if (refs.size === 0) return breadcrumbs;

  const resolved = await resolver([...refs]);

  return breadcrumbs.map((bc) => ({
    ...bc,
    href: ancestryHref(bc.sectionRef, bc.subpath ?? "", ancestry, resolved),
  }));
}
