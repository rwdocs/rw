import { Page } from "@playwright/test";

/**
 * Resolve a page's comment documentId the way the viewer does:
 * `${meta.sectionRef}#${meta.subpath}` from the page API. The e2e specs seed
 * comments via the REST API and must key them the same way the viewer queries,
 * which since #527 is the (sectionRef, subpath) composite — not the URL path.
 *
 * `urlPath` is the page's URL path without a leading slash (`""` for the
 * homepage, `"getting-started"`, `"billing/invoices"`, …).
 */
export async function resolveDocumentId(page: Page, urlPath: string): Promise<string> {
  return page.evaluate(
    async (p) => {
      const res = await fetch(`/_api/pages/${p}`);
      if (!res.ok) throw new Error(`GET /_api/pages/${p} -> ${res.status}`);
      const { meta } = await res.json();
      return `${meta.sectionRef}#${meta.subpath}`;
    },
    urlPath.replace(/^\//, ""),
  );
}
