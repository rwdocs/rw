import { test, expect } from "@playwright/test";

test.describe("Tabs", () => {
  test("shows a focus ring on a keyboard-focused tab, not a clicked one", async ({ page }) => {
    await page.goto("/tabs");

    const macos = page.getByRole("tab", { name: "macOS" });
    const linux = page.getByRole("tab", { name: "Linux" });

    await linux.click();
    await expect(linux).toBeFocused();
    await expect(linux).toHaveCSS("outline-style", "none");

    await page.keyboard.press("ArrowLeft");
    await expect(macos).toBeFocused();
    await expect(macos).toHaveCSS("outline-style", "solid");
    await expect(macos).toHaveCSS("outline-width", "2px");
  });
});
