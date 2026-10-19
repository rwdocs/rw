import { test, expect } from "@playwright/test";

test.describe("ToC sticky behavior", () => {
  test.use({ viewport: { width: 1400, height: 400 } });

  test("ToC stays visible at top when content is scrolled down", async ({ page }) => {
    await page.goto("/");

    const tocHeading = page.getByRole("heading", { name: "On this page", exact: true });
    await expect(tocHeading).toBeVisible();

    await page.evaluate(() => window.scrollBy(0, 1000));

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

    await expect(page.getByRole("heading", { level: 1 })).toContainText("Test Documentation");

    await expect(article.locator("#features")).toBeVisible();
    await expect(article.locator("#quick-start")).toBeVisible();
    await expect(article.locator("#code-example")).toBeVisible();
  });

  test("renders internal links correctly", async ({ page }) => {
    await page.goto("/");

    const gettingStartedLink = page.getByRole("article").getByRole("link", {
      name: "Getting Started",
    });
    await expect(gettingStartedLink).toBeVisible();
    await gettingStartedLink.click();

    await expect(page).toHaveURL(/\/getting-started$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Getting Started");
  });

  test("table of contents lists the headings and scrolls to one on click", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByRole("heading", { name: "On this page", exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "Features" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Quick Start" })).toBeVisible();

    await page.getByRole("link", { name: "Code Example" }).click();

    await expect(page.locator("#code-example")).toBeInViewport();
  });

  test("shows 404 page for missing content", async ({ page }) => {
    await page.goto("/nonexistent-page");

    await expect(page.locator("body")).toContainText(/not found/i);
  });

  test("internal links with relative paths work", async ({ page }) => {
    await page.goto("/advanced/plugins/custom");

    // The fixture link is `../../getting-started/installation.md`.
    const installationLink = page.getByRole("article").getByRole("link", { name: "Installation" });
    await expect(installationLink).toBeVisible();
    await installationLink.click();

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
