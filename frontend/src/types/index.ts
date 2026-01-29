/** Navigation tree item from GET /api/navigation */
export interface NavItem {
  title: string;
  path: string;
  /** Section type if this item is a section root. */
  section_type?: string;
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
  /** Display title. */
  title: string;
  /** Section type (e.g., "domain", "system"). */
  type: string;
}

/** Complete navigation tree with scope information */
export interface NavigationTree {
  items: NavItem[];
  /** Current scope info (null at root). */
  scope: ScopeInfo | null;
  /** Parent scope for back navigation (null at root or if no parent section). */
  parentScope: ScopeInfo | null;
}

/** Page metadata from GET /api/pages/{path} */
export interface PageMeta {
  title: string;
  path: string;
  sourceFile: string;
  lastModified: string; // ISO 8601
  description?: string;
  type?: string;
  vars?: Record<string, unknown>;
  /** Navigation scope path (without leading slash, empty for root scope). */
  navigationScope: string;
}

/** Breadcrumb navigation item */
export interface Breadcrumb {
  title: string;
  path: string;
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

/** Section from GET /api/sections */
export interface Section {
  title: string;
  path: string;
  type: string;
}

/** Response from GET /api/sections */
export interface SectionsResponse {
  sections: Section[];
}
