/** Section identity from the API. */
export interface SectionInfo {
  /** Section kind (e.g., "domain", "system"). */
  kind: string;
  /** Section namespace (e.g., "default", "payments"). */
  namespace: string;
  /** Section name — last path segment (e.g., "billing"). */
  name: string;
}

/** Navigation tree item from GET /_api/navigation */
export interface NavItem {
  title: string;
  path: string;
  /** Resolved external URL for cross-section navigation (bypasses prefixPath). */
  href?: string;
  /** Section identity if this item is a section root. */
  section?: SectionInfo;
  children?: NavItem[];
}

/** Group of navigation items with optional label. */
export interface NavGroup {
  /** Group label (null for ungrouped items). */
  label: string | null;
  /** Items in this group. */
  items: NavItem[];
}

/**
 * One rung of a section's ancestry chain: a section ref and the target path
 * expressed relative to that section's root. Chains run deepest-first (the
 * section itself, then ancestors, root last).
 */
export interface SectionAnchor {
  sectionRef: string;
  subpath: string;
}

/**
 * Every section ref mapped to its ordered ancestry chain, delivered once per
 * page/navigation response. A link's nearest `sectionRef` keys into this; the
 * viewer walks the chain to the first host-mapped ancestor to build a URL.
 */
export type SectionAncestry = Record<string, SectionAnchor[]>;

/** Information about a navigation scope. */
export interface ScopeInfo {
  /** URL path (with leading slash). */
  path: string;
  /** Resolved external URL for cross-section navigation (bypasses prefixPath). */
  href?: string;
  /** Display title. */
  title: string;
  /**
   * Section identity. The scope's own ref (with an empty subpath) is its key
   * into the navigation response's `sectionAncestry` map.
   */
  section: SectionInfo;
}

/** Complete navigation tree with scope information */
export interface NavigationTree {
  items: NavItem[];
  /** Current scope info (omitted at root). */
  scope?: ScopeInfo;
  /** Parent scope for back navigation (omitted at root or if no parent section). */
  parentScope?: ScopeInfo;
  /** Section ref → ancestry chain, for resolving nav/scope hrefs. */
  sectionAncestry?: SectionAncestry;
}

/** Page metadata from GET /_api/pages/{path} */
export interface PageMeta {
  title: string;
  path: string;
  sourceFile: string;
  lastModified: string; // ISO 8601
  description?: string;
  kind?: string;
  /** Section ref for this page's section (e.g., "domain:default/billing"). */
  sectionRef: string;
  /**
   * Page path relative to its section root. Stable across whole-section moves
   * (unlike `path`), so embedding hosts can key comments on
   * `(sectionRef, subpath)`.
   */
  subpath: string;
}

/** Breadcrumb navigation item */
export interface Breadcrumb {
  title: string;
  path: string;
  /** Resolved external URL for cross-section navigation (bypasses prefixPath). */
  href?: string;
  /**
   * Section ref of the nearest enclosing section — this crumb's key into the
   * page response's `sectionAncestry` map.
   */
  sectionRef?: string;
  /** This crumb's path relative to `sectionRef`'s scope root. */
  subpath?: string;
}

/** Table of contents entry */
export interface TocEntry {
  level: number; // 2-6 (h2-h6)
  title: string;
  id: string;
}

/** Page response from GET /_api/pages/{path} */
export interface PageResponse {
  meta: PageMeta;
  breadcrumbs: Breadcrumb[];
  toc: TocEntry[];
  content: string; // HTML
  /** Section ref → ancestry chain, for resolving breadcrumb and content-link hrefs. */
  sectionAncestry?: SectionAncestry;
}

/** API error response */
export interface ApiError {
  error: string;
  path?: string;
}

/** Server config from GET /_api/config */
export interface ConfigResponse {
  liveReloadEnabled: boolean;
  commentsEnabled?: boolean;
}
