import type { SectionInfo, SectionAncestry } from "../types";
import { encodePath } from "./encodePath";

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

/**
 * Join a section-relative ancestor subpath with a link's own subpath. A link to
 * a section root with a query has a subpath of just `?…`, which takes no slash.
 */
function joinSubpath(base: string, sub: string): string {
  if (!base || !sub) return base || sub;
  return sub.startsWith("?") ? `${base}${sub}` : `${base}/${sub}`;
}

/**
 * Join a resolved host base with a subpath, stripping a single trailing slash
 * from `base` first so a base that already ends in "/" doesn't produce a
 * doubled slash. Returns `base` unchanged when `rest` is empty, and appends a
 * query-only `rest` to it directly.
 */
function joinBase(base: string, rest: string): string {
  if (!rest) return base;
  return rest.startsWith("?") ? `${base}${rest}` : `${base.replace(/\/$/, "")}/${rest}`;
}

/**
 * Resolve a link's nearest `(sectionRef, subpath)` to a host URL by walking the
 * ref's ancestry chain (from the response's `sectionAncestry` map) to the
 * nearest host-mapped ancestor, then joining that ancestor's subpath with the
 * link's own. Every chain ends at the site-root ref, which the host is
 * required to map (see `SectionRefResolver`), so this only returns `undefined`
 * when the ref itself is absent from the ancestry map or the host violates
 * that contract — the caller then falls back to local routing.
 *
 * `subpath` is already URL-encoded, as `data-section-path` is; the ancestry
 * subpaths are page paths and are encoded here.
 */
export function ancestryHref(
  sectionRef: string,
  subpath: string,
  ancestry: SectionAncestry,
  resolved: Record<string, string>,
): string | undefined {
  const chain = ancestry[sectionRef];
  if (!chain) return undefined;
  for (const a of chain) {
    const base = resolved[a.sectionRef];
    // A JS host isn't type-checked against Record<string, string> at runtime
    // and may return null (or "") for a ref it declines to map; treat only a
    // non-empty string as mapped so those fall through to the next ancestor.
    if (typeof base === "string" && base !== "") {
      const rest = joinSubpath(encodePath(a.subpath), subpath);
      return joinBase(base, rest);
    }
  }
  return undefined;
}

/** The host's base URL for each section ref, asked for once per ref. */
export class SectionLinks {
  private readonly bases: Record<string, string> = {};

  constructor(private readonly resolver?: SectionRefResolver) {}

  get enabled(): boolean {
    return this.resolver !== undefined;
  }

  /** Ask the host about the refs in `ancestry`'s chains that it has not answered. */
  async ensure(ancestry: SectionAncestry): Promise<void> {
    const refs = new Set(
      Object.values(ancestry).flatMap((chain) => chain.map((a) => a.sectionRef)),
    );
    const missing = [...refs].filter((ref) => !Object.hasOwn(this.bases, ref));
    if (!this.resolver || missing.length === 0) return;
    try {
      const resolved = await this.resolver(missing);
      for (const ref of missing) this.bases[ref] = resolved[ref];
    } catch (e) {
      // Unanswered refs fall back to local routing and are asked again next time.
      if (import.meta.env.DEV) console.warn("[rw] section-ref resolution failed:", e);
    }
  }

  /** The host URL for `subpath` (URL-encoded) under `sectionRef`, if the host maps its chain. */
  href(sectionRef: string, subpath: string, ancestry: SectionAncestry): string | undefined {
    return ancestryHref(sectionRef, subpath, ancestry, this.bases);
  }
}

/**
 * Point internal links in rendered HTML content at the host's URLs.
 *
 * Each link carries a flat `data-section-ref` (its nearest section) and an
 * optional `data-section-path` (the target relative to that section).
 *
 * `roots` is a list rather than a single container because the links are not
 * all in one tree. Most are ordinary markdown internal links in the article
 * itself; diagram anchors (annotated by the renderer's `HtmlBackend`) instead
 * sit inside a `<rw-diagram>` shadow root, which `querySelectorAll` does not
 * pierce. Both kinds must be rewritten, so pass the article element *and* every
 * diagram shadow root under it (see `diagramShadowRoots`).
 */
export function rewriteSectionRefLinks(
  roots: ParentNode[],
  links: SectionLinks,
  basePath: string,
  ancestry: SectionAncestry,
): void {
  for (const root of roots) {
    for (const link of root.querySelectorAll<HTMLElement>("a[data-section-ref]")) {
      const sectionRef = link.getAttribute("data-section-ref") ?? "";
      const subpath = link.getAttribute("data-section-path") ?? "";
      // The backend keeps a link's "?…" in data-section-path but drops its "#…",
      // so the fragment is read back off the current href and re-appended.
      const cur = link.getAttribute("href") ?? "";
      const hashIdx = cur.indexOf("#");
      const hash = hashIdx >= 0 ? cur.slice(hashIdx) : "";

      // Last resort: not even the site-root ref is host-mapped, which the host
      // is required to do (see `SectionRefResolver`). Joining the link's own
      // subpath keeps distinct unmapped links off the bare mount root.
      const href = links.href(sectionRef, subpath, ancestry) ?? joinBase(basePath, subpath);
      link.setAttribute("href", href + hash);
    }
  }
}

const siteHrefs = new WeakMap<Element, string>();

/**
 * Without a host resolver, point section links at the viewer's own routes, as
 * nav links are. Their hrefs are site paths, which only coincide with host
 * URLs when the viewer is mounted at the origin root. A link `toHref` maps to
 * null keeps its site path.
 */
export function rewriteLocalLinks(
  roots: ParentNode[],
  toHref: (siteHref: string) => string | null,
): void {
  for (const root of roots) {
    for (const link of root.querySelectorAll<HTMLElement>("a[data-section-ref]")) {
      // A live reload can keep the nodes, already rewritten.
      let siteHref = siteHrefs.get(link);
      if (siteHref === undefined) {
        siteHref = link.getAttribute("href") ?? "";
        siteHrefs.set(link, siteHref);
      }
      link.setAttribute("href", toHref(siteHref) ?? siteHref);
    }
  }
}
