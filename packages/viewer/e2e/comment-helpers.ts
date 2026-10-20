import { expect, type Page } from "@playwright/test";

/**
 * All fixture servers share one SQLite store. Each spec owns a distinct page
 * to keep resolved history separate; grep before claiming one. Tests mutating
 * the same document use `e2e-comments:<documentId>` locks so cleanup cannot
 * resolve another test's in-flight threads, including across projects.
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

interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Viewport rect of `text`, which must sit within one text node of the article. */
export function textRect(page: Page, text: string): Promise<Rect> {
  return measureText(page, text, false);
}

/**
 * Click `text` in the article with the real mouse: highlights and selection
 * collapse respond to a pointer hit, not to an element `.click()`.
 */
export async function clickText(page: Page, text: string) {
  const r = await measureText(page, text, true);
  await page.mouse.click((r.left + r.right) / 2, (r.top + r.bottom) / 2);
}

function measureText(page: Page, text: string, forClick: boolean): Promise<Rect> {
  return page.getByRole("article").evaluate(
    async (article, { target, click }) => {
      const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) {
        const node = walker.currentNode as Text;
        const idx = node.data.indexOf(target);
        if (idx === -1) continue;
        const range = document.createRange();
        if (click) {
          // Scroll and measure in one evaluate, after letting the app's
          // rAF-deferred scrollIntoView calls (comment focus, deep-link reveal)
          // run: one landing later would move the text off the measured point.
          node.parentElement?.scrollIntoView({ block: "center" });
          await new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          );
          // One glyph, because the middle of a wrapped range can be off the text.
          range.setStart(node, idx + 1);
          range.setEnd(node, idx + 2);
        } else {
          range.setStart(node, idx);
          range.setEnd(node, idx + target.length);
        }
        const r = range.getBoundingClientRect();
        if (r.width === 0 && r.height === 0) {
          // Clicking (0, 0) would hit the viewport corner and fail far from the cause.
          throw new Error(`range for "${target}" collapsed to a zero-size rect`);
        }
        return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
      }
      throw new Error(`text "${target}" not found in article`);
    },
    { target: text, click: forClick },
  );
}

/**
 * Resolve a page's comment documentId from the page API, the key the viewer
 * lists and creates comments under (not the URL path).
 *
 * `urlPath` is the page's URL path without a leading slash (`""` for the
 * homepage, `"getting-started"`, `"billing/invoices"`, …).
 */
export async function resolveDocumentId(page: Page, urlPath: string): Promise<string> {
  return page.evaluate(
    async (p) => {
      const res = await fetch(`/_api/pages/${p}`);
      if (!res.ok) throw new Error(`GET /_api/pages/${p} -> ${res.status}`);
      const { meta } = (await res.json()) as { meta: { documentId: string } };
      return meta.documentId;
    },
    urlPath.replace(/^\//, ""),
  );
}

interface ApiSelector {
  type: string;
  exact?: string;
  prefix?: string;
  suffix?: string;
  start?: number;
  end?: number;
}

/**
 * The fields of a `/_api/comments` entry that the e2e suite reads.
 *
 * Deliberately partial — the specs assert on a handful of fields and the server
 * owns the rest. It exists so the one place that parses the response can name a
 * shape: `Response.json()` is `Promise<any>`, and without a cast here every
 * field read in every spec is unchecked.
 */
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

/** PATCH a comment's status. */
export async function setCommentStatus(
  page: Page,
  id: string,
  status: "open" | "resolved",
): Promise<void> {
  await page.evaluate(
    async ({ commentId, st }) => {
      const res = await fetch(`/_api/comments/${commentId}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ status: st }),
      });
      if (!res.ok) throw new Error(`PATCH /_api/comments/${commentId} -> ${res.status}`);
    },
    { commentId: id, st: status },
  );
}

/**
 * Clear a document's open comments, so state left by one test does not leak
 * into the next: resolve every unresolved thread and delete every reply. A
 * reply cannot be resolved, and resolving its thread leaves it in a
 * `status=open` listing, where it would shadow a later reply with the same body.
 */
export async function resolveAllComments(page: Page, documentId: string): Promise<void> {
  for (const comment of await fetchComments(page, documentId)) {
    if (comment.parentId) {
      await deleteComment(page, comment.id);
    } else if (comment.status !== "resolved") {
      await setCommentStatus(page, comment.id, "resolved");
    }
  }
}

async function deleteComment(page: Page, id: string): Promise<void> {
  await page.evaluate(async (commentId) => {
    const res = await fetch(`/_api/comments/${commentId}`, { method: "DELETE" });
    if (!res.ok) throw new Error(`DELETE /_api/comments/${commentId} -> ${res.status}`);
  }, id);
}

/** Wait until stored comments have loaded and anchored as article highlights. */
export async function waitForHighlights(page: Page) {
  await expect(page.getByRole("article").locator("rw-annotation").first()).toBeAttached({
    timeout: 10_000,
  });
}

/**
 * Load `path` with every stored comment anchored and no thread active. Opening
 * a thread mirrors `#comment-<id>` into the URL, and a bare reload would
 * re-activate it, so bounce through about:blank to land on the bare path.
 */
export async function reloadIdle(page: Page, path: string) {
  await page.goto("about:blank");
  await page.goto(path);
  await page.getByRole("article").waitFor();
  await page.getByRole("region", { name: "Comments" }).waitFor();
  await waitForHighlights(page);
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
