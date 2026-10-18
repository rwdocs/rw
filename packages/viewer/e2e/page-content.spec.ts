import { test, expect } from "@playwright/test";

test.describe("ToC sticky behavior", () => {
  test.use({ viewport: { width: 1400, height: 400 } });

  test("ToC stays visible at top when content is scrolled down", async ({ page }) => {
    await page.goto("/");

    // Wait for TOC to be visible
    const tocHeading = page.getByRole("heading", { name: "On this page", exact: true });
    await expect(tocHeading).toBeVisible();

    // Scroll the window down
    await page.evaluate(() => window.scrollBy(0, 1000));

    // The TOC should still be visible at the top after scrolling
    await expect(tocHeading).toBeInViewport();
  });
});

test.describe("Page Content", () => {
  // Wide enough for the ToC sidebar to render (breakpoint: 1304px)
  test.use({ viewport: { width: 1400, height: 720 } });

  test("renders markdown content with headings", async ({ page }) => {
    await page.goto("/");

    const article = page.getByRole("article");
    await expect(article).toBeVisible();

    // Should have page title (h1)
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Test Documentation");

    // Should have section headings (h2)
    await expect(article.locator("#features")).toBeVisible();
    await expect(article.locator("#quick-start")).toBeVisible();
    await expect(article.locator("#code-example")).toBeVisible();
  });

  test("renders code blocks with syntax highlighting", async ({ page }) => {
    await page.goto("/");

    // Should have code blocks
    const codeBlocks = page.locator("pre code");
    await expect(codeBlocks.first()).toBeVisible();

    // Code block should have language class for highlighting
    await expect(codeBlocks.first()).toHaveAttribute("class", /language-typescript/);

    // Should also have Python code block
    const pythonBlock = page.locator('pre code[class*="language-python"]');
    await expect(pythonBlock).toBeVisible();
    await expect(pythonBlock).toContainText("def greet");
  });

  test("renders internal links correctly", async ({ page }) => {
    await page.goto("/");

    const article = page.getByRole("article");

    // Should have internal links
    const gettingStartedLink = article.getByRole("link", { name: "Getting Started" });
    await expect(gettingStartedLink).toBeVisible();

    // Click internal link
    await gettingStartedLink.click();

    // Should navigate to the linked page
    await expect(page).toHaveURL(/\/getting-started$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Getting Started");
  });

  test("shows table of contents", async ({ page }) => {
    await page.goto("/");

    // ToC should be visible (has "On this page" heading)
    await expect(page.getByRole("heading", { name: "On this page", exact: true })).toBeVisible();

    // Should contain heading links
    await expect(page.getByRole("link", { name: "Features" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Quick Start" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Code Example" })).toBeVisible();
  });

  test("table of contents scrolls to section on click", async ({ page }) => {
    await page.goto("/");

    // Click on Code Example in ToC
    await page.getByRole("link", { name: "Code Example" }).click();

    // Code Example heading should be in viewport
    const codeExampleHeading = page.locator("#code-example");
    await expect(codeExampleHeading).toBeInViewport();
  });

  test("shows 404 page for missing content", async ({ page }) => {
    await page.goto("/nonexistent-page");

    // Should show not found message
    await expect(page.locator("body")).toContainText(/not found/i);
  });

  test("internal links with relative paths work", async ({ page }) => {
    await page.goto("/advanced/plugins/custom");

    const article = page.getByRole("article");

    // Click link to installation (uses ../../getting-started/installation.md)
    const installationLink = article.getByRole("link", { name: "Installation" });
    await expect(installationLink).toBeVisible();
    await installationLink.click();

    // Should navigate correctly
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
  });
});

test.describe("Pages under /api/", () => {
  // Wide enough for the ToC sidebar to render (breakpoint: 1304px)
  test.use({ viewport: { width: 1400, height: 720 } });

  // A doc page whose URL starts with /api/
  // must not collide with the server API namespace on a server round-trip.
  test("loads a page under /api/ on direct navigation and reload", async ({ page }) => {
    await page.goto("/api/endpoints");

    const article = page.getByRole("article");
    await expect(article).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Endpoints");

    // Reload hits the server fresh (no SPA shortcut) — must still resolve.
    await page.reload();
    await expect(article).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Endpoints");
  });

  // Unmatched paths under the reserved /_api/ prefix must 404, not fall through
  // to the SPA shell — otherwise API clients receive HTML with a 200 status.
  // Covers both the bare `/_api` segment and a path under the prefix.
  for (const apiPath of ["/_api", "/_api/does-not-exist"]) {
    test(`unknown ${apiPath} returns 404 with no HTML body`, async ({ page }) => {
      const response = await page.request.get(apiPath);
      expect(response.status()).toBe(404);
      expect(response.headers()["content-type"] ?? "").not.toContain("html");
    });
  }
});
