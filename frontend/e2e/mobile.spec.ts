import { test, expect } from "@playwright/test";

test.use({ viewport: { width: 390, height: 844 } }); // iPhone 13 dimensions

test.describe("Mobile Navigation", () => {
  test("hides desktop sidebar on mobile", async ({ page }) => {
    await page.goto("/");

    // Desktop aside should not be visible on mobile
    const desktopNav = page.locator("aside").first();
    await expect(desktopNav).toBeHidden();
  });

  test("shows hamburger menu button", async ({ page }) => {
    await page.goto("/");

    // Hamburger button should be visible
    const menuButton = page.getByRole("button", { name: "Open menu" });
    await expect(menuButton).toBeVisible();
  });

  test("opens mobile drawer on menu click", async ({ page }) => {
    await page.goto("/");

    // Click hamburger menu
    await page.getByRole("button", { name: "Open menu" }).click();

    // Mobile drawer should appear
    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Should show navigation items
    await expect(drawer.getByRole("link", { name: "Getting Started" })).toBeVisible();
    await expect(drawer.getByRole("link", { name: "API Reference" })).toBeVisible();
  });

  test("closes drawer on navigation", async ({ page }) => {
    await page.goto("/");

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();

    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Expand Getting Started
    const gettingStartedLink = drawer.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();

    // Click Installation link
    await drawer.getByRole("link", { name: "Installation" }).click();

    // Drawer should close after navigation
    await expect(drawer).toBeHidden();

    // Should have navigated
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
  });

  test("closes drawer on escape key", async ({ page }) => {
    await page.goto("/");

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();

    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Press escape
    await page.keyboard.press("Escape");

    // Drawer should close
    await expect(drawer).toBeHidden();
  });

  test("closes drawer on overlay click", async ({ page }) => {
    await page.goto("/");

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();

    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Click overlay at a position outside the drawer (drawer is 280px wide from left)
    // Using force: true to bypass the drawer intercepting clicks
    const overlay = page.getByRole("button", { name: "Close menu" }).first();
    await overlay.click({ position: { x: 350, y: 400 }, force: true });

    // Drawer should close
    await expect(drawer).toBeHidden();
  });

  test("content is readable on mobile", async ({ page }) => {
    await page.goto("/");

    const article = page.locator("article");
    await expect(article).toBeVisible();

    // Content should be readable
    await expect(article).toContainText("Test Documentation");
    await expect(article).toContainText("Features");
  });

  test("table of contents is hidden on mobile", async ({ page }) => {
    await page.goto("/");

    // ToC sidebar should be hidden on mobile (shown only on lg: screens)
    const tocHeading = page.getByText("On this page");
    await expect(tocHeading).toBeHidden();
  });

  test("code blocks are scrollable on mobile", async ({ page }) => {
    await page.goto("/");

    // Code blocks should be present and visible
    const codeBlock = page.locator("pre").first();
    await expect(codeBlock).toBeVisible();

    // Code block should have overflow-x-auto for horizontal scrolling
    await expect(codeBlock).toHaveCSS("overflow-x", "auto");
  });

  test("navigation works across multiple pages on mobile", async ({ page }) => {
    await page.goto("/");

    // Open drawer and navigate
    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.locator("aside.fixed");

    // Expand Getting Started
    const gsLink = drawer.getByRole("link", { name: "Getting Started" });
    await gsLink.locator("..").getByRole("button").click();
    await drawer.getByRole("link", { name: "Installation" }).click();

    // Verify navigation
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
    await expect(page.locator("article h1")).toContainText("Installation");

    // Navigate to another page
    await page.getByRole("button", { name: "Open menu" }).click();
    await page.locator("aside.fixed").getByRole("link", { name: "API Reference" }).click();

    // Verify second navigation
    await expect(page).toHaveURL(/\/api$/);
    await expect(page.locator("article h1")).toContainText("API Reference");
  });

  test("breadcrumbs work on mobile", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Breadcrumbs should be visible
    const breadcrumbs = page.locator("nav ol");
    await expect(breadcrumbs).toBeVisible();

    // Click Home breadcrumb
    await breadcrumbs.getByRole("link", { name: "Home" }).click();

    // Should navigate home
    await expect(page.locator("article")).toContainText("Test Documentation");
  });
});
