import { test, expect } from "@playwright/test";

test.describe("Empty navigation - single-page site", () => {
  test("desktop: no sidebar, article still renders", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByRole("article")).toContainText("Single Page Site");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toHaveCount(0);
  });

  test("mobile: no hamburger menu button", async ({ page }) => {
    await page.setViewportSize({ width: 480, height: 800 });
    await page.goto("/");

    await expect(page.getByRole("article")).toContainText("Single Page Site");
    await expect(page.getByRole("button", { name: "Open menu" })).toHaveCount(0);
  });
});
