import { test, expect } from "@playwright/test";
import {
  createComment,
  resolveAllComments,
  resolveDocumentId,
  waitForHighlights,
} from "./comment-helpers";
import { hostOwnsScroller } from "./host-scroller";

test.use({ viewport: { width: 1280, height: 800 } });
// Shares its page with comment-resolve-badge.spec.ts, so it takes that lock.
const commentsLock = { lock: "e2e-comments:section:default/root#getting-started/installation" };

const PAGE_PATH = "./getting-started/installation";
const PAGE_URL = "getting-started/installation";
// Far apart, so stepping from one to the other moves the thread a long way.
const ANCHORS = ["install the platform on your system", "operating systems and architectures"];

test.describe("Embedded comment prev/next", commentsLock, () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(PAGE_PATH);
    await page.getByRole("article").waitFor();
    const documentId = await resolveDocumentId(page, PAGE_URL);
    await resolveAllComments(page, documentId);
    for (const [i, quote] of ANCHORS.entries()) {
      await createComment(page, { documentId, body: `scroll seed ${i}`, quote });
    }
  });

  for (const scroller of ["window", "host"] as const) {
    test(`Next stays under the cursor when the ${scroller} scrolls`, async ({ page }) => {
      await page.reload();
      if (scroller === "host") await hostOwnsScroller(page);
      await waitForHighlights(page);
      await expect(page.getByRole("article").locator("rw-annotation")).toHaveCount(2);

      await page.keyboard.press("n");
      const sidebar = page.getByRole("complementary", { name: "Comments" });
      await expect(sidebar.getByText("1 / 2", { exact: true })).toBeVisible();
      const next = sidebar.getByRole("button", { name: "Next comment" });
      const before = await next.boundingBox();

      await next.click();
      await expect(sidebar.getByText("2 / 2", { exact: true })).toBeVisible();
      const after = await next.boundingBox();
      expect(after!.y).toBeCloseTo(before!.y, 0);
    });
  }
});
