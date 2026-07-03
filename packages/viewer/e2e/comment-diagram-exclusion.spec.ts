import { test, expect } from "@playwright/test";
import { selectText } from "./comment-helpers";

test.describe("diagram comment exclusion", () => {
  test("selecting a diagram label shows no Add comment popover", async ({ page }) => {
    await page.goto("/diagram");
    await page.getByRole("article").waitFor();

    // Positive control: a prose selection DOES offer the popover.
    await selectText(page, "raw diagram figure");
    await expect(page.getByRole("button", { name: "Add comment" })).toBeVisible();

    // Clear, then select the SVG label — the popover must not appear.
    await page.mouse.click(1, 1);
    await selectText(page, "Big diagram");
    await expect(page.getByRole("button", { name: "Add comment" })).toBeHidden();
  });
});
