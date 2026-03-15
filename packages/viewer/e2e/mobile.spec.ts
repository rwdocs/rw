import { test, expect } from "@playwright/test";

test.use({ viewport: { width: 390, height: 844 } }); // iPhone 13 dimensions

test.describe("Mobile Navigation", () => {
  test("hides desktop sidebar on mobile", async ({ page }) => {
    await page.goto("/");

    // Desktop aside should not be visible on mobile
    const desktopNav = page.getByRole("complementary", { name: "Sidebar" });
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
    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    // Should show navigation items
    await expect(drawer.getByRole("link", { name: "Getting Started" })).toBeVisible();
    await expect(drawer.getByRole("link", { name: "API Reference" })).toBeVisible();
  });

  test("closes drawer on navigation", async ({ page }) => {
    await page.goto("/");

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();

    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
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

    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
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

    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    // Click outside the drawer on the backdrop overlay
    // Drawer is 280px wide on left, so click to the right of it
    const drawerBox = await drawer.boundingBox();
    expect(drawerBox).not.toBeNull();
    await page.mouse.click(drawerBox!.x + drawerBox!.width + 50, drawerBox!.y + 100);

    // Drawer should close
    await expect(drawer).toBeHidden();
  });

  test("content is readable on mobile", async ({ page }) => {
    await page.goto("/");

    const article = page.getByRole("article");
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
    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });

    // Expand Getting Started
    const gsLink = drawer.getByRole("link", { name: "Getting Started" });
    await gsLink.locator("..").getByRole("button").click();
    await drawer.getByRole("link", { name: "Installation" }).click();

    // Verify navigation
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");

    // Navigate to another page
    await page.getByRole("button", { name: "Open menu" }).click();
    await page
      .getByRole("complementary", { name: "Mobile navigation" })
      .getByRole("link", { name: "API Reference" })
      .click();

    // Verify second navigation
    await expect(page).toHaveURL(/\/api$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Reference");
  });

  test("breadcrumbs work on mobile", async ({ page }) => {
    await page.goto("/getting-started/installation");

    // Breadcrumbs should be visible
    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(breadcrumbs).toBeVisible();

    // Click Home breadcrumb
    await breadcrumbs.getByRole("link", { name: "Home" }).click();

    // Should navigate home
    await expect(page.getByRole("article")).toContainText("Test Documentation");
  });

  test("shows breadcrumbs in mobile header, not in content area", async ({ page }) => {
    await page.goto("/getting-started/installation");

    const header = page.locator("header");
    const headerBreadcrumbs = header.getByRole("navigation", { name: "Breadcrumb" });
    await expect(headerBreadcrumbs).toBeVisible();

    // Desktop breadcrumbs in the content area should be hidden
    const desktopBreadcrumbs = page.locator(".layout-desktop-breadcrumbs");
    await expect(desktopBreadcrumbs).toBeHidden();
  });

  test("shows TOC button in mobile header", async ({ page }) => {
    await page.goto("/");

    const header = page.locator("header");
    const tocButton = header.getByRole("button", { name: "Table of contents" });
    await expect(tocButton).toBeVisible();

    // Content area TOC popover should be hidden on mobile
    const contentTocPopover = page.locator(".layout-toc-popover");
    await expect(contentTocPopover).toBeHidden();
  });

  test("mobile header TOC button opens popover and navigates to heading", async ({ page }) => {
    await page.goto("/");

    const header = page.locator("header");
    const tocButton = header.getByRole("button", { name: "Table of contents" });
    await tocButton.click();

    // Popover should appear with TOC links
    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await expect(popover).toBeVisible();

    // Click a heading link
    await popover.getByRole("link", { name: "Code Example" }).click();

    // Popover should close after navigation
    await expect(popover).toBeHidden();

    // Target heading should be visible (not behind the sticky header)
    const heading = page.locator("#code-example");
    await expect(heading).toBeInViewport();
  });

  test("heading is not hidden behind mobile header after TOC navigation", async ({ page }) => {
    await page.goto("/");

    // Open TOC and click a heading
    const header = page.locator("header");
    await header.getByRole("button", { name: "Table of contents" }).click();
    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await popover.getByRole("link", { name: "Code Example" }).click();

    // The heading's top should be below the mobile header's bottom
    const headerBox = await header.boundingBox();
    const headingBox = await page.locator("#code-example").boundingBox();
    expect(headerBox).not.toBeNull();
    expect(headingBox).not.toBeNull();
    expect(headingBox!.y).toBeGreaterThanOrEqual(headerBox!.y + headerBox!.height - 1);
  });
});
