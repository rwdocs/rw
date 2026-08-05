import { test, expect } from "@playwright/test";
import { createComment, resolveAllComments, resolveDocumentId } from "./comment-helpers";

// This spec's own page — see `comment-helpers.ts`.
//
// Distinct from `comment-stale-list.spec.ts`'s "advanced/plugins/custom-extensions",
// which is a sibling document, not this one.
const PAGE_URL = "advanced/plugins/custom";

// The live-reload server (see `playwright.config.ts`): its change broadcast is
// the only thing that refreshes comments without a navigation, and a navigation
// resets the store's rows first.
//
// Shared with `diagram-isolation.spec.ts`. Its fixture rewrite cannot reach this
// page — content reloads are matched against the open document's path — but
// comment broadcasts carry no path, so a future spec here that counts comment
// requests would see the ones this spec makes.
const LIVE_ORIGIN = "http://127.0.0.1:8084";

test.use({ viewport: { width: 1400, height: 800 } });

/**
 * A deleted row is filtered out of every list the server sends, so each refresh
 * is another chance to lose it and the Restore control with it. Surviving the
 * first is not enough — hence two, each confirmed to have landed by the comment
 * that triggered it appearing.
 */
test("a deleted reply keeps its Restore control across later refreshes", async ({ page }) => {
  await page.goto(`${LIVE_ORIGIN}/${PAGE_URL}`);
  const docId = await resolveDocumentId(page, PAGE_URL);
  await resolveAllComments(page, docId);
  const threadId = await createComment(page, {
    documentId: docId,
    body: "Thread with a reply",
    selectors: [],
  });
  await createComment(page, {
    documentId: docId,
    parentId: threadId,
    body: "Reply to delete",
    selectors: [],
  });

  await page.goto(`${LIVE_ORIGIN}/${PAGE_URL}`);
  const section = page.getByRole("region", { name: "Comments" });
  await expect(section).toContainText("Reply to delete");

  const restore = section.getByRole("button", { name: "Restore" });
  await section.getByRole("button", { name: "Delete" }).click();
  await expect(restore).toBeVisible();

  // Each of these broadcasts a change, and the refresh it triggers answers with
  // a list the deleted reply is not in. Waiting for the new comment to render is
  // waiting for that answer to have been applied.
  for (const body of ["First later change", "Second later change"]) {
    await createComment(page, { documentId: docId, body, selectors: [] });
    await expect(section).toContainText(body);

    await expect(restore).toBeVisible();
    await expect(section).toContainText("Reply to delete");
  }
});
