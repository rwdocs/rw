/** Navigation tree item from GET /api/navigation */
export interface NavItem {
  title: string;
  path: string;
  children?: NavItem[];
}

/** Complete navigation tree */
export interface NavigationTree {
  items: NavItem[];
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
