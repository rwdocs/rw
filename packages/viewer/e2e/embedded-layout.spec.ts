import { test, expect } from "@playwright/test";

test.describe("Embedded Layout", () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test("content area does not have internal scroll", async ({ page }) => {
    await page.goto("/");

    const scrollArea = page.getByTestId("content-area");
    await expect(scrollArea).toHaveCSS("overflow-y", "visible");
  });

  test("sidebar is sticky", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toBeVisible();
    await expect(sidebar).toHaveCSS("position", "sticky");
  });

  test("viewer does not overflow its container horizontally", async ({ page }) => {
    await page.goto("/wide-content");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Wide Content");

    const viewer = page.getByTestId("viewer-root");
    const overflow = await viewer.evaluate((el) => ({
      offsetWidth: (el as HTMLElement).offsetWidth,
      scrollWidth: el.scrollWidth,
    }));
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.offsetWidth);
  });

  test("shell header scrolls away when scrolling down", async ({ page }) => {
    await page.goto("/");

    const shellHeader = page.getByRole("banner");
    await expect(shellHeader).toBeInViewport();

    await page.evaluate(() => window.scrollBy(0, 300));

    await expect(shellHeader).not.toBeInViewport();
  });

  test("navigating to a new page scrolls window to top", async ({ page }) => {
    await page.goto("/");

    // Scroll down first
    await page.evaluate(() => window.scrollBy(0, 300));
    const scrolledY = await page.evaluate(() => window.scrollY);
    expect(scrolledY).toBeGreaterThan(0);

    // Navigate to another page via sidebar
    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    const gsLink = sidebar.getByRole("link", { name: "Getting Started" });
    const expandButton = gsLink.locator("..").getByRole("button");
    await expandButton.click();
    await sidebar.getByRole("link", { name: "Installation" }).click();

    // Wait for navigation
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");

    // Window should have scrolled to top
    const finalY = await page.evaluate(() => window.scrollY);
    expect(finalY).toBe(0);
  });

  test("navigation sidebar shows items and expands", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toBeVisible();
    await expect(sidebar.getByRole("link", { name: "Getting Started" })).toBeVisible();

    const gsLink = sidebar.getByRole("link", { name: "Getting Started" });
    await gsLink.locator("..").getByRole("button").click();
    await expect(sidebar.getByRole("link", { name: "Installation" })).toBeVisible();
  });

  test("page content renders correctly", async ({ page }) => {
    await page.goto("/");

    const article = page.getByRole("article");
    await expect(article).toBeVisible();
    await expect(article).toContainText("Test Documentation");
  });

  test("breadcrumbs display on inner pages", async ({ page }) => {
    await page.goto("/getting-started/installation");

    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(breadcrumbs).toBeVisible();
    await expect(breadcrumbs.getByRole("link", { name: "Home" })).toBeVisible();
  });

  test("navigating between pages works", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await sidebar.getByRole("link", { name: "API Reference" }).click();

    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Reference");
  });

  test("table of contents is visible on wide viewport", async ({ page }) => {
    // The TOC shows at container-width >= 1224px. The embedded preview shell
    // has a 250px sidebar, so we need viewport >= 1224 + 250 = 1474px.
    await page.setViewportSize({ width: 1600, height: 800 });
    await page.goto("/");

    const toc = page.getByRole("complementary", { name: "Page outline" });
    await expect(toc).toBeVisible();
    await expect(toc.getByText("On this page")).toBeVisible();
  });

  test("hash fragment scrolls to heading", async ({ page }) => {
    await page.goto("/#code-example");

    const heading = page.locator("#code-example");
    await expect(heading).toBeInViewport();
  });
});

test.describe("Embedded Layout - Mobile", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("mobile drawer covers full viewport height", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    const panel = drawer.getByTestId("mobile-drawer-panel");
    const panelBox = await panel.boundingBox();
    expect(panelBox).not.toBeNull();
    expect(panelBox!.height).toBeGreaterThanOrEqual(800);
  });

  test("mobile drawer stays within viewer container horizontally", async ({ page }) => {
    await page.goto("/");

    const viewer = page.getByTestId("viewer-root");
    const viewerBox = await viewer.boundingBox();
    expect(viewerBox).not.toBeNull();

    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("complementary", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    const drawerBox = await drawer.boundingBox();
    expect(drawerBox).not.toBeNull();
    expect(drawerBox!.x).toBeGreaterThanOrEqual(viewerBox!.x);
  });

  test("backdrop stays within viewer container horizontally", async ({ page }) => {
    await page.goto("/");

    const viewer = page.getByTestId("viewer-root");
    const viewerBox = await viewer.boundingBox();
    expect(viewerBox).not.toBeNull();

    await page.getByRole("button", { name: "Open menu" }).click();
    const backdrop = page.getByRole("button", { name: "Close menu" }).first();
    await expect(backdrop).toBeVisible();

    const backdropBox = await backdrop.boundingBox();
    expect(backdropBox).not.toBeNull();
    expect(backdropBox!.x).toBeGreaterThanOrEqual(viewerBox!.x);
  });

  test("hash fragment heading is not hidden behind mobile header", async ({ page }) => {
    await page.goto("/getting-started/installation");

    await expect(page.getByRole("button", { name: "Open menu" })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");
    const heading = page.getByRole("heading", { name: "Requirements" });
    await expect(heading).toBeAttached();

    await heading.evaluate((el) => el.scrollIntoView({ behavior: "auto" }));

    const header = page.getByRole("banner", { name: "Mobile header" });
    const headerBox = await header.boundingBox();
    const headingBox = await heading.boundingBox();
    expect(headerBox).not.toBeNull();
    expect(headingBox).not.toBeNull();
    expect(headingBox!.y).toBeGreaterThanOrEqual(headerBox!.y + headerBox!.height);
  });
});
