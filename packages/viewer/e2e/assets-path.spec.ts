import { test, expect } from "@playwright/test";

// The viewer's own bundle is served from /assets/, so a page there must not be
// mistaken for a missing static file.
test.describe("Pages under /assets/", () => {
  test("opening the page by URL renders it", async ({ page }) => {
    await page.goto("/assets/guide");

    await expect(page.getByRole("article")).toContainText("The asset guide page.");
  });

  test("clicking its nav item renders it", async ({ page }) => {
    await page.goto("/assets");
    const aside = page.getByRole("complementary", { name: "Sidebar" });

    await aside.getByRole("link", { name: "Asset Guide", exact: true }).click();

    await expect(page).toHaveURL(/\/assets\/guide$/);
    await expect(page.getByRole("article")).toContainText("The asset guide page.");
  });
});
