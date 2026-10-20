import { test, expect } from "@playwright/test";

test.use({ viewport: { width: 390, height: 844 } });

test.describe("Embedded mobile drawer", () => {
  test("closes with Escape and restores focus to the hamburger", async ({ page }) => {
    await page.goto("/");

    const hamburger = page.getByRole("button", { name: "Open menu" });
    await hamburger.click();

    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    await expect(drawer).toBeFocused();

    // From a control inside the panel, not just the panel itself.
    await page.keyboard.press("Tab");
    await expect(drawer.getByRole("button", { name: "Close menu" })).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(drawer).toBeHidden();
    await expect(hamburger).toBeFocused();
  });
});
