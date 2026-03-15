import { test, expect } from "@playwright/test";

test.describe("Navigation", () => {
  test("homepage shows content", async ({ page }) => {
    await page.goto("/");

    // Home page should load and show content
    await expect(page.getByRole("article")).toContainText("Test Documentation");
  });

  test("shows navigation sidebar with top-level sections", async ({ page }) => {
    await page.goto("/");

    // Navigation sidebar should be visible
    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await expect(aside).toBeVisible();

    // Should show top-level navigation items
    await expect(aside.getByRole("link", { name: "Getting Started" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "API Reference" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Advanced Topics" })).toBeVisible();
  });

  test("expands navigation tree on click", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    // Click the expand button next to Getting Started
    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();

    // Should show child items
    await expect(aside.getByRole("link", { name: "Installation" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Configuration" })).toBeVisible();
  });

  test("collapses expanded navigation on second click", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    // Expand Getting Started section
    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();

    // Verify expanded
    await expect(aside.getByRole("link", { name: "Installation" })).toBeVisible();

    // Click again to collapse
    await expandButton.click();

    // Children should be hidden
    await expect(aside.getByRole("link", { name: "Installation" })).toBeHidden();
  });

  test("navigates to page on click", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    // Expand Getting Started section
    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();

    // Click on Installation
    await aside.getByRole("link", { name: "Installation" }).click();

    // Should navigate to installation page
    await expect(page).toHaveURL(/\/getting-started\/installation$/);

    // Page content should load
    await expect(page.getByRole("article")).toContainText("Install via npm");
  });

  test("shows breadcrumbs on nested pages", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Breadcrumbs should show path
    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(breadcrumbs).toBeVisible();

    // Check breadcrumb content
    await expect(breadcrumbs).toContainText("Home");
    await expect(breadcrumbs).toContainText("Getting Started");
  });

  test("breadcrumb links navigate correctly", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Click on Getting Started breadcrumb
    await page
      .getByRole("navigation", { name: "Breadcrumb" })
      .getByRole("link", { name: "Getting Started" })
      .click();

    // Should navigate to getting started section
    await expect(page).toHaveURL(/\/getting-started$/);
    await expect(page.getByRole("article")).toContainText("This guide will help you get started");
  });

  test("breadcrumb Home link navigates to homepage", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Click on Home breadcrumb
    await page
      .getByRole("navigation", { name: "Breadcrumb" })
      .getByRole("link", { name: "Home" })
      .click();

    // Should navigate to home
    await expect(page.getByRole("article")).toContainText("Test Documentation");
  });

  test("highlights active navigation item", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    // Expand and navigate to Installation
    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();
    await aside.getByRole("link", { name: "Installation" }).click();

    // Wait for navigation
    await expect(page).toHaveURL(/\/getting-started\/installation$/);

    // Installation link should have active styling (text-blue-700)
    const installLink = aside.getByRole("link", { name: "Installation" });
    await expect(installLink).toHaveClass(/text-blue-700/);
  });

  test("deep navigation works (3 levels)", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    // Expand Advanced Topics
    const advancedLink = aside.getByRole("link", { name: "Advanced Topics" });
    const advancedExpand = advancedLink.locator("..").getByRole("button");
    await advancedExpand.click();

    // Expand Plugin Development
    const pluginsLink = aside.getByRole("link", { name: "Plugin Development" });
    const pluginsExpand = pluginsLink.locator("..").getByRole("button");
    await pluginsExpand.click();

    // Click Custom Plugin Guide
    await aside.getByRole("link", { name: "Custom Plugin Guide" }).click();

    // Should navigate to deep nested page
    await expect(page).toHaveURL(/\/advanced\/plugins\/custom$/);
    await expect(page.getByRole("article")).toContainText(
      "Step-by-step guide to creating a custom plugin",
    );
  });

  test("scrolls to top when navigating to a different page", async ({ page }) => {
    // Use a small viewport so page content requires scrolling
    await page.setViewportSize({ width: 1280, height: 200 });
    await page.goto("/getting-started/installation");

    // Wait for content to load
    await expect(page.getByRole("article")).toContainText("Install via npm");

    // Scroll the window down
    await page.evaluate(() => window.scrollBy(0, 500));
    const scrolledY = await page.evaluate(() => window.scrollY);
    expect(scrolledY).toBeGreaterThan(0);

    // Click a different page in the navigation
    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await aside.getByRole("link", { name: "Configuration" }).click();
    await expect(page).toHaveURL(/\/getting-started\/configuration$/);

    // Window should be scrolled to top
    const newScrollY = await page.evaluate(() => window.scrollY);
    expect(newScrollY).toBe(0);
  });

  test("auto-expands navigation to current page", async ({ page }) => {
    // Navigate directly to a nested page
    await page.goto("/advanced/plugins/custom");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    // The path to the current page should be expanded
    await expect(aside.getByRole("link", { name: "Custom Plugin Guide" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Plugin Development" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Advanced Topics" })).toBeVisible();
  });
});
