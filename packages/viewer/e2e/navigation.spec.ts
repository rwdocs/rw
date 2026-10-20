import { test, expect } from "@playwright/test";

test.describe("Navigation", () => {
  test("shows navigation sidebar with top-level sections, and no rw wordmark", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await expect(aside).toBeVisible();

    await expect(aside.getByRole("link", { name: "Getting Started" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "API Reference" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Advanced Topics" })).toBeVisible();

    // ...and no "rw" of its own. Anchored, or the match is a substring one and
    // any title containing "rw" trips it; padded, because a regex matches raw
    // text, and a sibling node leaves a space the anchors would otherwise miss.
    await expect(aside.getByText(/^\s*rw\s*$/i)).toHaveCount(0);
    await expect(aside.getByRole("img", { name: /^rw$/i })).toHaveCount(0);
  });

  test("expands navigation on click and collapses it on a second click", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    const gettingStartedLink = aside.getByRole("link", { name: "Getting Started" });
    const expandButton = gettingStartedLink.locator("..").getByRole("button");
    await expandButton.click();

    await expect(aside.getByRole("link", { name: "Installation" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Configuration" })).toBeVisible();

    await expandButton.click();

    await expect(aside.getByRole("link", { name: "Installation" })).toBeHidden();
  });

  test("breadcrumb links navigate correctly", async ({ page }) => {
    await page.goto("/getting-started/installation");

    await page
      .getByRole("navigation", { name: "Breadcrumb" })
      .getByRole("link", { name: "Getting Started" })
      .click();

    await expect(page).toHaveURL(/\/getting-started$/);
    await expect(page.getByRole("article")).toContainText("This guide will help you get started");
  });

  test("breadcrumb root link navigates to homepage", async ({ page }) => {
    await page.goto("/getting-started/installation");

    await page
      .getByRole("navigation", { name: "Breadcrumb" })
      .getByRole("link", { name: "Test Documentation" })
      .click();

    await expect(page).toHaveURL("/");
    await expect(page.getByRole("article")).toContainText("Test Documentation");
  });

  test("deep navigation works (3 levels)", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    const advancedLink = aside.getByRole("link", { name: "Advanced Topics" });
    await advancedLink.locator("..").getByRole("button").click();

    const pluginsLink = aside.getByRole("link", { name: "Plugin Development" });
    await pluginsLink.locator("..").getByRole("button").click();

    await aside.getByRole("link", { name: "Custom Plugin Guide" }).click();

    await expect(page).toHaveURL(/\/advanced\/plugins\/custom$/);
    await expect(page.getByRole("article")).toContainText(
      "Step-by-step guide to creating a custom plugin",
    );
  });

  test("scrolls to top when navigating to a different page", async ({ page }) => {
    // Small enough that the page content scrolls.
    await page.setViewportSize({ width: 1280, height: 200 });
    await page.goto("/getting-started/installation");

    await expect(page.getByRole("article")).toContainText("Install via npm");

    await page.evaluate(() => window.scrollBy(0, 500));
    const scrolledY = await page.evaluate(() => window.scrollY);
    expect(scrolledY).toBeGreaterThan(0);

    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await aside.getByRole("link", { name: "Configuration" }).click();
    await expect(page).toHaveURL(/\/getting-started\/configuration$/);

    const newScrollY = await page.evaluate(() => window.scrollY);
    expect(newScrollY).toBe(0);
  });

  test("auto-expands navigation to current page", async ({ page }) => {
    await page.goto("/advanced/plugins/custom");

    const aside = page.getByRole("complementary", { name: "Sidebar" });

    await expect(aside.getByRole("link", { name: "Custom Plugin Guide" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Plugin Development" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Advanced Topics" })).toBeVisible();
  });

  test("navigates to a page on click and marks its nav link current", async ({ page }) => {
    await page.goto("/");

    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await aside
      .getByRole("link", { name: "Getting Started" })
      .locator("..")
      .getByRole("button")
      .click();
    const active = aside.getByRole("link", { name: "Installation" });
    await active.click();
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
    await expect(page.getByRole("article")).toContainText("Install via npm");
    await expect(active).toHaveAttribute("aria-current", "page");

    // A non-active leaf sibling under the same section must not carry it,
    // confirming aria-current marks only the current page, not the whole branch.
    const inactive = aside.getByRole("link", { name: "Configuration" });
    await expect(inactive).not.toHaveAttribute("aria-current", "page");
  });

  test("desktop nav landmark has an accessible name", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("navigation", { name: "Documentation" })).toBeVisible();
  });
});
