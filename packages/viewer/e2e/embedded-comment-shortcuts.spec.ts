import { test, expect, type Page } from "@playwright/test";
import {
  createComment,
  reloadIdle,
  resolveAllComments,
  resolveDocumentId,
} from "./comment-helpers";

// Wide enough that the active thread opens in the comment sidebar.
test.use({ viewport: { width: 1400, height: 800 } });

/** Whether the viewer consumed the last `n`, read by a listener added after its own. */
async function watchN(page: Page) {
  await page.evaluate(() => {
    addEventListener("keydown", (event) => {
      if (event.key === "n") document.body.dataset.nPrevented = String(event.defaultPrevented);
    });
  });
  return () => page.evaluate(() => document.body.dataset.nPrevented);
}

test.describe(
  "Embedded comment shortcuts",
  { lock: "e2e-comments:section:default/root#api" },
  () => {
    test("leave a key pressed in the host alone and work inside the viewer", async ({ page }) => {
      await page.goto("/api");
      const documentId = await resolveDocumentId(page, "api");
      await resolveAllComments(page, documentId);
      await createComment(page, {
        documentId,
        body: "Shortcut scope",
        quote: "Complete API reference documentation",
      });
      await reloadIdle(page, "/api");
      const nPrevented = await watchN(page);
      const announcement = page.getByRole("status", { includeHidden: true });

      // The preview shell's own control, outside the viewer's mount root.
      await page.getByRole("button", { name: /^Theme:/ }).focus();
      await page.keyboard.press("n");
      expect(await nPrevented()).toBe("false");
      await expect(announcement).toHaveText("");

      await page.getByRole("article").getByRole("link", { name: "Endpoints Reference" }).focus();
      await page.keyboard.press("n");
      expect(await nPrevented()).toBe("true");
      await expect(announcement).toContainText("Comment 1 of 1");
    });
  },
);
