/** Section identity from the API. */
export interface SectionInfo {
  /** Section kind (e.g., "domain", "system"). */
  kind: string;
  /** Section name — last path segment (e.g., "billing"). */
  name: string;
}

/** Navigation tree item from GET /api/navigation */
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

/** Information about a navigation scope. */
export interface ScopeInfo {
  /** URL path (with leading slash). */
  path: string;
  /** Resolved external URL for cross-section navigation (bypasses prefixPath). */
  href?: string;
  /** Display title. */
  title: string;
  /** Section identity. */
  section: SectionInfo;
}

/** Complete navigation tree with scope information */
export interface NavigationTree {
  items: NavItem[];
  /** Current scope info (omitted at root). */
  scope?: ScopeInfo;
  /** Parent scope for back navigation (omitted at root or if no parent section). */
  parentScope?: ScopeInfo;
}

/** Page metadata from GET /api/pages/{path} */
export interface PageMeta {
  title: string;
  path: string;
  sourceFile: string;
  lastModified: string; // ISO 8601
  description?: string;
  kind?: string;
  vars?: Record<string, unknown>;
  /** Navigation scope path (without leading slash, empty for root scope). */
  navigationScope: string;
}

/** Breadcrumb navigation item */
export interface Breadcrumb {
  title: string;
  path: string;
  /** Resolved external URL for cross-section navigation (bypasses prefixPath). */
  href?: string;
  /** Section identity if this breadcrumb's path matches a section. */
  section?: SectionInfo;
}

/** Table of contents entry */
export interface TocEntry {
  level: number; // 2-6 (h2-h6)
  title: string;
  id: string;
}

/** Page response from GET /api/pages/{path} */
export interface PageResponse {
  meta: PageMeta;
  breadcrumbs: Breadcrumb[];
  toc: TocEntry[];
  content: string; // HTML
}

/** API error response */
export interface ApiError {
  error: string;
  path?: string;
}

/** Server config from GET /api/config */
export interface ConfigResponse {
  liveReloadEnabled: boolean;
}
