import { test, expect, type Page } from "@playwright/test";
import { resolveDocumentId } from "./comment-helpers";

// Wide viewport so the right comment sidebar is visible.
test.use({ viewport: { width: 1400, height: 800 } });
test.describe.configure({ mode: "serial" });

// Dedicated page so these tests never share comment rows with other specs.
const PAGE_PATH = "/getting-started/configuration";
const PAGE_URL = "getting-started/configuration";
const ANCHOR_ONE = "configure the platform";
const ANCHOR_TWO = "Later sources override earlier ones";

/** POST a comment to the REST API from the browser context. Returns its id. */
async function postComment(page: Page, payload: Record<string, unknown>): Promise<string> {
  return page.evaluate(async (body) => {
    const res = await fetch("/_api/comments", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`create comment failed: ${res.status}`);
    return (await res.json()).id as string;
  }, payload);
}

/** Seed an inline comment anchored to `anchorText` (a passage present verbatim
 *  in the article) via the REST API, so it loads as an inline thread in the
 *  right sidebar — the surface where the reused-form bug lived. */
async function seedInlineComment(
  page: Page,
  documentId: string,
  anchorText: string,
  body: string,
): Promise<string> {
  return postComment(page, {
    documentId,
    body,
    selectors: [{ type: "TextQuoteSelector", exact: anchorText, prefix: "", suffix: "" }],
  });
}

async function resolveAllComments(page: Page, documentId: string) {
  await page.evaluate(async (docId) => {
    const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}`);
    const comments = await res.json();
    for (const c of comments) {
      if (c.status === "open") {
        await fetch(`/_api/comments/${c.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ status: "resolved" }),
        });
      }
    }
  }, documentId);
}

async function waitForHighlights(page: Page) {
  await expect(async () => {
    const count = await page.evaluate(
      () => document.querySelectorAll("article rw-annotation").length,
    );
    expect(count).toBeGreaterThan(0);
  }).toPass({ timeout: 10000 });
}

/** Reload to a true idle state (no active thread, no hash). */
async function reloadIdle(page: Page) {
  await page.goto("about:blank");
  await page.goto(PAGE_PATH);
  await page.getByRole("article").waitFor();
  await page.getByRole("region", { name: "Comments" }).waitFor();
  await waitForHighlights(page);
}

/** Open an inline thread by clicking its in-article highlighted text. Only open
 *  comments are highlighted here, so each anchor passage maps to one thread.
 *  Clicks at the glyph's screen coordinates (the highlight's click handler
 *  responds to a real pointer hit, not an element `.click()`). */
async function openThreadByText(page: Page, text: string) {
  // Scroll and measure in one evaluate. Splitting them meant the rect was read
  // a round-trip after the scroll, so it could be stale; the fixed 100ms that
  // used to bridge the gap held on an idle machine and not under parallel load.
  // `scrollIntoView` is instant here (nothing sets `scroll-behavior: smooth`),
  // and reading `getBoundingClientRect` forces the layout flush.
  const coords = await page.evaluate(async (targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const idx = (walker.currentNode.textContent ?? "").indexOf(targetText);
      if (idx === -1) continue;
      const scrollRange = document.createRange();
      scrollRange.setStart(walker.currentNode, idx);
      scrollRange.setEnd(walker.currentNode, idx + targetText.length);
      scrollRange.startContainer.parentElement?.scrollIntoView({ block: "center" });
      // Let any scrollIntoView the app deferred to rAF (comment focus, deep-link
      // reveal) run before measuring. Measuring in the same task as our own
      // scroll is correct — scrollIntoView is synchronous — but a frame the app
      // already queued would otherwise land between this rect and the click.
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const clickRange = document.createRange();
      clickRange.setStart(walker.currentNode, idx + 1);
      clickRange.setEnd(walker.currentNode, idx + 2);
      const rect = clickRange.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        // A re-wrap during the frames we just waited on collapses the range;
        // clicking (0, 0) would hit the viewport corner and fail somewhere far
        // from the cause.
        throw new Error(`range for "${targetText}" collapsed to a zero-size rect`);
      }
      return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    }
    throw new Error(`text "${targetText}" not found`);
  }, text);
  await page.mouse.click(coords.x, coords.y);
}

test.describe("Reply drafts are scoped per thread", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(PAGE_PATH);
    const docId = await resolveDocumentId(page, PAGE_URL);
    await resolveAllComments(page, docId);
    await page.reload();
    await page.getByRole("article").waitFor();
    await page.getByRole("region", { name: "Comments" }).waitFor();
  });

  test("a draft in one thread does not leak to another and is restored on return", async ({
    page,
  }) => {
    const docId = await resolveDocumentId(page, PAGE_URL);
    await seedInlineComment(page, docId, ANCHOR_ONE, "thread one root");
    await seedInlineComment(page, docId, ANCHOR_TWO, "thread two root");
    await reloadIdle(page);

    const sidebar = page.getByRole("complementary", { name: "Comments" });
    const replyBox = () => sidebar.getByPlaceholder("Write a reply...");

    // Open the first thread and type a draft (don't submit).
    await openThreadByText(page, ANCHOR_ONE);
    await expect(sidebar).toContainText("thread one root");
    await replyBox().fill("draft for thread one");

    // Switch to the second thread: its reply box must be empty (no leak).
    await openThreadByText(page, ANCHOR_TWO);
    await expect(sidebar).toContainText("thread two root");
    await expect(replyBox()).toHaveValue("");

    // Return to the first thread: the draft is restored. Assert on the textarea
    // VALUE — the {#key} remount intentionally resets transient submit/failed
    // state, so only the draft text is preserved.
    await openThreadByText(page, ANCHOR_ONE);
    await expect(sidebar).toContainText("thread one root");
    await expect(replyBox()).toHaveValue("draft for thread one");
  });
});
