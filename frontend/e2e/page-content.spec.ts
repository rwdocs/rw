import { test, expect } from "@playwright/test";

test.describe("Page Content", () => {
  test("renders markdown content", async ({ page }) => {
    await page.goto("/index");

    // Article should contain rendered content
    const article = page.locator("article");
    await expect(article).toBeVisible();

    // Should have page title
    await expect(article).toContainText("Docstage");

    // Should have features section
    await expect(article).toContainText("Features");
  });

  test("renders code blocks with syntax highlighting", async ({ page }) => {
    await page.goto("/index");

    // Should have code blocks
    const codeBlock = page.locator("pre code");
    await expect(codeBlock.first()).toBeVisible();

    // Code block should have language class for highlighting
    await expect(codeBlock.first()).toHaveAttribute("class", /language-/);
  });

  test("renders internal links correctly", async ({ page }) => {
    await page.goto("/index");

    // Documentation section has internal links
    const usageLink = page.locator("article").getByRole("link", { name: "Usage Guide" });
    await expect(usageLink).toBeVisible();

    // Click internal link
    await usageLink.click();

    // Should navigate
    await expect(page).toHaveURL(/\/usage$/);
  });

  test("shows table of contents", async ({ page }) => {
    await page.goto("/index");

    // ToC should be visible (has "On this page" heading)
    await expect(page.getByText("On this page")).toBeVisible();

    // Should contain heading buttons (ToC uses buttons, not links)
    await expect(page.getByRole("button", { name: "Features" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Quick Start" })).toBeVisible();
  });

  test("table of contents buttons scroll to section", async ({ page }) => {
    await page.goto("/index");

    // Click on Features in ToC
    await page.getByRole("button", { name: "Features" }).click();

    // Features heading should be near the top of the viewport
    const featuresHeading = page.locator("#features");
    await expect(featuresHeading).toBeInViewport();
  });

  test("shows 404 page for missing content", async ({ page }) => {
    await page.goto("/nonexistent-page");

    // Should show not found message
    await expect(page.locator("body")).toContainText(/not found/i);
  });

  test("handles nested pages", async ({ page }) => {
    await page.goto("/architecture/rust-core");

    // Should load nested page content
    const article = page.locator("article");
    await expect(article).toBeVisible();
    await expect(article).toContainText("Rust");
  });
});
