import { test, expect, Page } from "@playwright/test";

// Wide viewport so the right comment sidebar is visible.
test.use({ viewport: { width: 1400, height: 800 } });
test.describe.configure({ mode: "serial" });

// These tests run on a dedicated page (its own documentId) so they never share
// comment rows with comments.spec.ts — the two spec files run in parallel and
// both create/resolve comments, so using the same documentId would let one
// file's resolveAll close the other's in-flight comments. The intro line of
// this page is the passage we anchor inline comments to.
const PAGE_PATH = "/getting-started/configuration";
const PAGE_DOC_ID = "getting-started/configuration";
const ANCHOR_TEXT = "configure the platform";

/** Select a text range inside the article and trigger the selection popover. */
async function selectText(page: Page, text: string) {
  await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const fullText = article.textContent ?? "";
    const startInDoc = fullText.indexOf(targetText);
    if (startInDoc === -1) throw new Error(`text "${targetText}" not found`);
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
    if (!startNode || !endNode) throw new Error(`couldn't build range for "${targetText}"`);
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

async function createInlineComment(page: Page, targetText: string, body: string) {
  await selectText(page, targetText);
  await page.getByRole("button", { name: "Add comment" }).click();
  const sidebar = page.getByRole("complementary", { name: "Comments" });
  await sidebar.getByPlaceholder("Write a comment...").fill(body);
  await sidebar.getByRole("button", { name: "Comment", exact: true }).click();
  await expect(sidebar.getByPlaceholder("Write a comment...")).not.toBeVisible();
}

async function createPageComment(page: Page, body: string) {
  const section = page.getByRole("region", { name: "Comments" });
  await section.getByPlaceholder("Write a comment...").fill(body);
  await section.getByRole("button", { name: "Comment", exact: true }).click();
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

/** data-comment-id of the inline highlight currently marked active, if any. */
function activeHighlightId(page: Page) {
  return page.evaluate(
    () =>
      document
        .querySelector("article rw-annotation[data-active='true']")
        ?.getAttribute("data-comment-id") ?? null,
  );
}

/** The visually-hidden aria-live region. It is sr-only (clipped), so it is
 *  outside Playwright's default accessibility tree — query it with includeHidden. */
function liveRegion(page: Page) {
  return page.getByRole("status", { includeHidden: true });
}

/** Reload the page so the comment store starts fresh (activeId null = idle),
 *  with all DB-persisted comments loaded and inline highlights anchored. This
 *  mirrors how a reviewer actually arrives: open the page, then press a key. */
async function reloadIdle(page: Page) {
  await page.reload();
  await page.getByRole("article").waitFor();
  await page.getByRole("region", { name: "Comments" }).waitFor();
  await waitForHighlights(page);
}

test.describe("Comment keyboard navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(PAGE_PATH);
    await resolveAllComments(page, PAGE_DOC_ID);
    await page.reload();
    await page.getByRole("article").waitFor();
    // The page-comments <section> only mounts once the config request has
    // flipped comments on. Selecting text before then misses the one-shot
    // mouseup, so the "Add comment" popover never appears. Wait for it.
    await page.getByRole("region", { name: "Comments" }).waitFor();
  });

  test("n from idle opens the first comment and scrolls it into view", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "first inline");
    await reloadIdle(page);

    await page.keyboard.press("n");

    await expect(page.getByRole("complementary", { name: "Comments" })).toBeVisible();
    expect(await activeHighlightId(page)).not.toBeNull();
    await expect(liveRegion(page)).toContainText("Comment 1 of 1");
    // The active highlight is scrolled into view (centered) on the jump.
    await expect(page.locator("article rw-annotation[data-active='true']")).toBeInViewport();
  });

  test("n steps through inline then page comments and wraps", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    await createPageComment(page, "page level one");
    await expect(page.getByRole("region", { name: "Comments" })).toContainText("page level one");
    await reloadIdle(page);

    await page.keyboard.press("n"); // idle → first (inline, 1 of 2)
    await expect(liveRegion(page)).toContainText("Comment 1 of 2");
    const firstActive = await activeHighlightId(page);
    expect(firstActive).not.toBeNull();

    await page.keyboard.press("n"); // → page comment (2 of 2)
    await expect(liveRegion(page)).toContainText("Comment 2 of 2");
    // The page comment is not an inline highlight, so no active highlight now.
    expect(await activeHighlightId(page)).toBeNull();

    await page.keyboard.press("n"); // wraps → inline (1 of 2)
    await expect(liveRegion(page)).toContainText("Comment 1 of 2");
    expect(await activeHighlightId(page)).toBe(firstActive);
  });

  test("p from idle jumps to the last comment", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    await createPageComment(page, "page level last");
    await expect(page.getByRole("region", { name: "Comments" })).toContainText("page level last");
    await reloadIdle(page);

    await page.keyboard.press("p"); // idle → last (the page comment)

    await expect(liveRegion(page)).toContainText("Comment 2 of 2");
    expect(await activeHighlightId(page)).toBeNull();
  });

  test("typing n in the comment form does not navigate", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    await reloadIdle(page);

    const textarea = page
      .getByRole("region", { name: "Comments" })
      .getByPlaceholder("Write a comment...");
    await textarea.click();
    await textarea.pressSequentially("nano notes");

    await expect(textarea).toHaveValue("nano notes");
    expect(await activeHighlightId(page)).toBeNull();
    await expect(liveRegion(page)).toHaveText("");
  });
});
