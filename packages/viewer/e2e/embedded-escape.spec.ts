import { test, expect, type Page } from "@playwright/test";
import { createComment, resolveAllComments, resolveDocumentId } from "./comment-helpers";

// The preview shell's own control, outside the viewer's mount root.
const hostControl = (page: Page) => page.getByRole("button", { name: /^Theme:/ });

test.describe("Embedded Escape", () => {
  test.describe("TOC popover", () => {
    test.use({ viewport: { width: 1100, height: 800 } });

    test("leaves Escape from the host alone and closes on Escape inside", async ({ page }) => {
      await page.goto("./");

      const trigger = page.getByRole("button", { name: "Table of contents" });
      await trigger.click();
      const popover = page.getByRole("navigation", { name: "Table of contents" });
      await expect(popover).toBeVisible();

      await hostControl(page).focus();
      await page.keyboard.press("Escape");
      await expect(popover).toBeVisible();
      await expect(hostControl(page)).toBeFocused();

      await popover.getByRole("link").first().focus();
      await page.keyboard.press("Escape");
      await expect(popover).toBeHidden();
      await expect(trigger).toBeFocused();
    });
  });

  test.describe("comment popover", { lock: "e2e-comments:section:default/root#api" }, () => {
    // Below the comments breakpoint, so the thread opens in the popover.
    test.use({ viewport: { width: 700, height: 900 } });

    async function openThread(page: Page) {
      await page.goto("./api");
      const documentId = await resolveDocumentId(page, "api");
      await resolveAllComments(page, documentId);
      await createComment(page, {
        documentId,
        body: "Escape scope",
        quote: "Complete API reference documentation",
      });
      await page.reload();

      await page.locator("rw-annotation").first().click();
      const popover = page.getByRole("group", { name: "Comments" });
      await expect(popover).toBeVisible();
      return popover;
    }

    test("leaves Escape from the host alone and closes on Escape inside", async ({ page }) => {
      const popover = await openThread(page);

      await hostControl(page).focus();
      await page.keyboard.press("Escape");
      await expect(popover).toBeVisible();
      await expect(hostControl(page)).toBeFocused();

      await popover.getByRole("button", { name: "Copy link" }).focus();
      await page.keyboard.press("Escape");
      await expect(popover).toBeHidden();
    });

    // A tap on a highlight leaves focus on <body>, which no host field owns.
    test("closes on Escape right after tapping the highlight", async ({ page }) => {
      const popover = await openThread(page);

      await page.keyboard.press("Escape");
      await expect(popover).toBeHidden();
    });
  });
});
