import type { PageMeta } from "../../types";

/**
 * Stable comment key for a page.
 *
 * Keyed on `(sectionRef, subpath)` rather than the URL `path`, so moving or
 * remounting a whole section (sectionRef unchanged) does not orphan its
 * comments. `subpath` is `""` for a section-root page, yielding a key like
 * `domain:default/billing#`. The key is opaque to the comment store.
 */
export function documentIdFor(meta: PageMeta): string {
  return `${meta.sectionRef}#${meta.subpath}`;
}
