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

  test("drawer panel stays within container bounds when container is shorter than viewport", async ({
    page,
  }) => {
    await page.goto("/");

    // Simulate embedded mode: constrain the viewer container so it doesn't
    // fill the full viewport (e.g., host app has a footer below).
    await page.getByTestId("viewer-root").evaluate((el) => {
      el.style.height = "500px";
      el.style.overflow = "hidden";
    });

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
    const panel = drawer.getByTestId("mobile-drawer-panel");
    await expect(panel).toBeVisible();

    // The panel bottom should not extend past the container bottom
    const [panelBottom, containerBottom] = await Promise.all([
      panel.evaluate((el) => el.getBoundingClientRect().bottom),
      page.getByTestId("viewer-root").evaluate((el) => el.getBoundingClientRect().bottom),
    ]);

    expect(panelBottom).toBeLessThanOrEqual(containerBottom);
  });

  test("drawer panel height updates when container resizes without viewport change", async ({
    page,
  }) => {
    await page.goto("/");

    // Constrain the container to simulate embedded mode
    await page.getByTestId("viewer-root").evaluate((el) => {
      el.style.height = "600px";
      el.style.overflow = "hidden";
    });

    // Open drawer and read initial panel height
    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
    const panel = drawer.getByTestId("mobile-drawer-panel");
    await expect(panel).toBeVisible();

    const initialHeight = await panel.evaluate((el) => el.getBoundingClientRect().height);

    // Shrink the container (NOT the viewport)
    await page.getByTestId("viewer-root").evaluate((el) => {
      el.style.height = "400px";
    });

    // Panel height should adapt to the smaller container
    await expect(async () => {
      const newHeight = await panel.evaluate((el) => el.getBoundingClientRect().height);
      expect(newHeight).toBeLessThan(initialHeight);
    }).toPass({ timeout: 2000 });
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
});
