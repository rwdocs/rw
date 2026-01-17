import { test, expect } from "@playwright/test";

test.describe("Navigation", () => {
  test("homepage shows content", async ({ page }) => {
    await page.goto("/");

    // Home page should load and show content
    await expect(page.locator("article")).toContainText("Test Documentation");
  });

  test("shows navigation sidebar with top-level sections", async ({ page }) => {
    await page.goto("/");

    // Navigation sidebar should be visible
    const aside = page.locator("aside").first();
    await expect(aside).toBeVisible();

    // Should show top-level navigation items
    await expect(aside.getByRole("link", { name: "Getting Started" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "API Reference" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Advanced Topics" })).toBeVisible();
  });

  test("expands navigation tree on click", async ({ page }) => {
    await page.goto("/");

    const aside = page.locator("aside").first();

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

    const aside = page.locator("aside").first();

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

    const aside = page.locator("aside").first();

    // Expand Getting Started section
    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();

    // Click on Installation
    await aside.getByRole("link", { name: "Installation" }).click();

    // Should navigate to installation page
    await expect(page).toHaveURL(/\/getting-started\/installation$/);

    // Page content should load
    await expect(page.locator("article")).toContainText("Install via npm");
  });

  test("shows breadcrumbs on nested pages", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Breadcrumbs should show path
    const breadcrumbNav = page.locator("nav ol");
    await expect(breadcrumbNav).toBeVisible();

    // Check breadcrumb content
    await expect(breadcrumbNav).toContainText("Home");
    await expect(breadcrumbNav).toContainText("Getting Started");
  });

  test("breadcrumb links navigate correctly", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Click on Getting Started breadcrumb
    await page.locator("nav ol").getByRole("link", { name: "Getting Started" }).click();

    // Should navigate to getting started section
    await expect(page).toHaveURL(/\/getting-started$/);
    await expect(page.locator("article")).toContainText("This guide will help you get started");
  });

  test("breadcrumb Home link navigates to homepage", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Click on Home breadcrumb
    await page.locator("nav ol").getByRole("link", { name: "Home" }).click();

    // Should navigate to home
    await expect(page.locator("article")).toContainText("Test Documentation");
  });

  test("highlights active navigation item", async ({ page }) => {
    await page.goto("/");

    const aside = page.locator("aside").first();

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

    const aside = page.locator("aside").first();

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
    await expect(page.locator("article")).toContainText(
      "Step-by-step guide to creating a custom plugin",
    );
  });

  test("auto-expands navigation to current page", async ({ page }) => {
    // Navigate directly to a nested page
    await page.goto("/advanced/plugins/custom");

    const aside = page.locator("aside").first();

    // The path to the current page should be expanded
    await expect(aside.getByRole("link", { name: "Custom Plugin Guide" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Plugin Development" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Advanced Topics" })).toBeVisible();
  });

  test("consecutive navigation clicks update page content", async ({ page }) => {
    // Regression test: consecutive clicks on same/different nav items should update content
    await page.goto("/getting-started");
    await expect(page.locator("article h1")).toContainText("Getting Started");

    const aside = page.locator("aside").first();

    // Navigate to different page
    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();
    await aside.getByRole("link", { name: "Installation" }).click();
    await expect(page.locator("article h1")).toContainText("Installation");

    // Navigate back to Getting Started
    await aside.getByRole("link", { name: "Getting Started" }).click();
    await expect(page.locator("article h1")).toContainText("Getting Started");

    // Navigate again to Installation
    await aside.getByRole("link", { name: "Installation" }).click();
    await expect(page.locator("article h1")).toContainText("Installation");
  });
});
