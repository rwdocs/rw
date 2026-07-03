import { Page } from "@playwright/test";

/**
 * Select `text` inside the article and release with a synthetic mouseup, which
 * is what drives the Add-comment popover. Builds the Range from
 * `article.textContent` — which includes inlined SVG diagram-label text — so it
 * can target prose or a diagram label.
 */
export async function selectText(page: Page, text: string) {
  await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const fullText = article.textContent ?? "";
    const startInDoc = fullText.indexOf(targetText);
    if (startInDoc === -1) throw new Error(`text "${targetText}" not found in article`);
    const endInDoc = startInDoc + targetText.length;

    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    let offset = 0;
    let startNode: Text | null = null;
    let startOffset = 0;
    let endNode: Text | null = null;
    let endOffset = 0;
    while (walker.nextNode()) {
      const node = walker.currentNode as Text;
      const len = node.data.length;
      if (!startNode && offset + len > startInDoc) {
        startNode = node;
        startOffset = startInDoc - offset;
      }
      if (startNode && offset + len >= endInDoc) {
        endNode = node;
        endOffset = endInDoc - offset;
        break;
      }
      offset += len;
    }
    if (!startNode || !endNode) {
      throw new Error(`couldn't build range for "${targetText}"`);
    }

    const range = document.createRange();
    range.setStart(startNode, startOffset);
    range.setEnd(endNode, endOffset);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);

    const rect = range.getBoundingClientRect();
    article.dispatchEvent(
      new MouseEvent("mouseup", {
        bubbles: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
  }, text);
}

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
