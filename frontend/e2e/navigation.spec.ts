import { test, expect } from "@playwright/test";

test.describe("Navigation", () => {
  test("homepage shows content", async ({ page }) => {
    await page.goto("/");

    // Home page should load and show content (may redirect or stay at /)
    await expect(page.locator("article")).toContainText("Docstage");
  });

  test("shows navigation sidebar", async ({ page }) => {
    await page.goto("/index");

    // Navigation sidebar should be visible (aside element on desktop)
    const aside = page.locator("aside").first();
    await expect(aside).toBeVisible();

    // Should show top-level navigation items
    await expect(aside.getByRole("link", { name: "Usage" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Architecture" })).toBeVisible();
  });

  test("expands navigation tree on click", async ({ page }) => {
    await page.goto("/index");

    const aside = page.locator("aside").first();

    // Click the expand button (it's the button before the Usage link, in same div)
    const usageLink = aside.getByRole("link", { name: "Usage" });
    // The button is a sibling in the same parent div
    const expandButton = usageLink.locator("..").getByRole("button");
    await expandButton.click();

    // Should show child items
    await expect(aside.getByRole("link", { name: "Server" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Diagrams" })).toBeVisible();
  });

  test("navigates to page on click", async ({ page }) => {
    await page.goto("/index");

    const aside = page.locator("aside").first();

    // Expand Usage section
    const usageLink = aside.getByRole("link", { name: "Usage" });
    const expandButton = usageLink.locator("..").getByRole("button");
    await expandButton.click();

    // Click on Server
    await aside.getByRole("link", { name: "Server" }).click();

    // Should navigate to server page
    await expect(page).toHaveURL(/\/usage\/server$/);

    // Page content should load
    await expect(page.locator("article")).toContainText("Server");
  });

  test("shows breadcrumbs", async ({ page }) => {
    await page.goto("/usage/server");

    // Breadcrumbs should show path
    const breadcrumbNav = page.locator("nav ol");
    await expect(breadcrumbNav).toBeVisible();

    // Check breadcrumb content
    await expect(breadcrumbNav).toContainText("Home");
    await expect(breadcrumbNav).toContainText("Usage");
  });

  test("breadcrumb links work", async ({ page }) => {
    await page.goto("/usage/server");

    // Click on Home breadcrumb
    await page.locator("nav ol").getByRole("link", { name: "Home" }).click();

    // Should navigate to home (/ or /index)
    await expect(page.locator("article")).toContainText("Docstage");
  });
});
