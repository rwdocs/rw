import { type Page } from "@playwright/test";

/**
 * Helpers for the comment specs, and the rule they all live under: **each spec
 * owns a fixture page no other spec touches.** All four `rw serve` fixtures keep
 * their data beside their config, so they share one SQLite store, and outside CI
 * the spec files run against it in parallel. Grep before claiming a page.
 */

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
      const { meta } = (await res.json()) as { meta: { sectionRef: string; subpath: string } };
      return `${meta.sectionRef}#${meta.subpath}`;
    },
    urlPath.replace(/^\//, ""),
  );
}

/**
 * The fields of a `/_api/comments` entry that the e2e suite reads.
 *
 * Deliberately partial — the specs assert on a handful of fields and the server
 * owns the rest. It exists so the one place that parses the response can name a
 * shape: `Response.json()` is `Promise<any>`, and without a cast here every
 * field read in every spec is unchecked.
 */
interface ApiSelector {
  type: string;
  exact?: string;
  prefix?: string;
  suffix?: string;
  start?: number;
  end?: number;
}

export interface ApiComment {
  id: string;
  body: string;
  status: string;
  parentId: string | null;
  selectors: ApiSelector[];
}

/** GET the comments for `documentId`, optionally filtered by status. */
export async function fetchComments(
  page: Page,
  documentId: string,
  status?: "open" | "resolved",
): Promise<ApiComment[]> {
  return page.evaluate(
    async ({ docId, st }) => {
      const query = new URLSearchParams({ documentId: docId });
      if (st) query.set("status", st);
      const res = await fetch(`/_api/comments?${query.toString()}`);
      if (!res.ok) throw new Error(`GET /_api/comments -> ${res.status}`);
      return (await res.json()) as ApiComment[];
    },
    { docId: documentId, st: status },
  );
}

/** POST a comment and return its id. */
export async function createComment(page: Page, payload: Record<string, unknown>): Promise<string> {
  return page.evaluate(async (body) => {
    const res = await fetch("/_api/comments", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`POST /_api/comments -> ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }, payload);
}

/**
 * Resolve every unresolved comment on a document, so state left by one test
 * does not leak into the next. `open` and `resolved` are the only statuses the
 * server emits, so this is the whole set.
 */
export async function resolveAllComments(page: Page, documentId: string): Promise<void> {
  await page.evaluate(async (docId) => {
    const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}`);
    if (!res.ok) throw new Error(`GET /_api/comments -> ${res.status}`);
    const comments = (await res.json()) as { id: string; status: string }[];
    for (const comment of comments) {
      if (comment.status === "resolved") continue;
      const patch = await fetch(`/_api/comments/${comment.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ status: "resolved" }),
      });
      if (!patch.ok) {
        throw new Error(`PATCH /_api/comments/${comment.id} -> ${patch.status}`);
      }
    }
  }, documentId);
}

/**
 * Unwrap an `Array.prototype.find` result, failing with a named error instead
 * of yielding `undefined`. Dereferencing the raw result would throw a bare
 * TypeError several lines later, and `?.` would silently compare `undefined`
 * to `undefined` in assertions that read a field off two comments.
 */
export function found<T>(value: T | undefined, what: string): T {
  if (value === undefined) throw new Error(`expected to find ${what}`);
  return value;
}
