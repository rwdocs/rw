import { test, expect, type Page } from "@playwright/test";
import {
  clickText,
  createComment,
  reloadIdle,
  resolveAllComments,
  resolveDocumentId,
} from "./comment-helpers";

// Wide viewport so the right comment sidebar is visible.
test.use({ viewport: { width: 1400, height: 800 } });
const commentsLock = { lock: "e2e-comments:section:default/root#advanced/plugins" };

// This spec's own page — see `comment-helpers.ts`.
const PAGE_PATH = "/advanced/plugins";
const PAGE_URL = "advanced/plugins";
const ANCHOR_ONE = "develop custom plugins";
const ANCHOR_TWO = "step-by-step instructions";

/** Seed an inline comment anchored to `anchorText` (a passage present verbatim
 *  in the article) via the REST API, so it loads as an inline thread in the
 *  right sidebar — the surface where the reused-form bug lived. */
async function seedInlineComment(
  page: Page,
  documentId: string,
  anchorText: string,
  body: string,
): Promise<string> {
  return createComment(page, {
    documentId,
    body,
    selectors: [{ type: "TextQuoteSelector", exact: anchorText, prefix: "", suffix: "" }],
  });
}

test.describe("Reply drafts are scoped per thread", commentsLock, () => {
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
    await reloadIdle(page, PAGE_PATH);

    const sidebar = page.getByRole("complementary", { name: "Comments" });
    const replyBox = () => sidebar.getByPlaceholder("Write a reply...");

    await clickText(page, ANCHOR_ONE);
    await expect(sidebar).toContainText("thread one root");
    await replyBox().fill("draft for thread one");

    await clickText(page, ANCHOR_TWO);
    await expect(sidebar).toContainText("thread two root");
    await expect(replyBox()).toHaveValue("");

    // Assert on the textarea VALUE — the {#key} remount intentionally resets
    // transient submit/failed state, so only the draft text is preserved.
    await clickText(page, ANCHOR_ONE);
    await expect(sidebar).toContainText("thread one root");
    await expect(replyBox()).toHaveValue("draft for thread one");
  });
});
