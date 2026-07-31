import { test, expect } from "@playwright/test";

// Viewport wide enough for desktop sidebar (>=952px) but narrow enough
// to hide the TOC sidebar (<1304px), so the popover button appears.
test.use({ viewport: { width: 1100, height: 800 } });

test.describe("TOC Popover", () => {
  test("shows popover button when TOC sidebar is hidden", async ({ page }) => {
    await page.goto("/");

    const button = page.getByRole("button", { name: "Table of contents" });
    await expect(button).toBeVisible();

    // Full TOC sidebar should be hidden at this width
    const tocSidebar = page.getByRole("complementary", { name: "Page outline" });
    await expect(tocSidebar).toBeHidden();
  });

  test("hides popover button when TOC sidebar is visible", async ({ page }) => {
    // Use a wide viewport where the full TOC sidebar is shown
    await page.setViewportSize({ width: 1400, height: 800 });
    await page.goto("/");

    const tocSidebar = page.getByRole("complementary", { name: "Page outline" });
    await expect(tocSidebar).toBeVisible();

    const button = page.getByRole("button", { name: "Table of contents" });
    await expect(button).toBeHidden();
  });

  test("opens popover on button click", async ({ page }) => {
    await page.goto("/");

    const button = page.getByRole("button", { name: "Table of contents" });
    await button.click();

    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await expect(popover).toBeVisible();
    await expect(popover).toMatchAriaSnapshot(`
      - navigation "Table of contents":
        - heading "On this page" [level=3]
        - list:
          - listitem:
            - link "Features":
              - /url: "#features"
          - listitem:
            - link "Quick Start":
              - /url: "#quick-start"
          - listitem:
            - link "Code Example":
              - /url: "#code-example"
          - listitem:
            - link "Learn More":
              - /url: "#learn-more"
    `);
  });

  test("sets aria-expanded on toggle", async ({ page }) => {
    await page.goto("/");

    const button = page.getByRole("button", { name: "Table of contents" });
    await expect(button).toHaveAttribute("aria-expanded", "false");

    await button.click();
    await expect(button).toHaveAttribute("aria-expanded", "true");

    await button.click();
    await expect(button).toHaveAttribute("aria-expanded", "false");
  });

  test("closes popover on Escape key", async ({ page }) => {
    await page.goto("/");

    const button = page.getByRole("button", { name: "Table of contents" });
    await button.click();

    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await expect(popover).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(popover).toBeHidden();
  });

  test("closes popover on click outside", async ({ page }) => {
    await page.goto("/");

    const button = page.getByRole("button", { name: "Table of contents" });
    await button.click();

    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await expect(popover).toBeVisible();

    // Click on the article content area (outside the popover)
    await page.getByRole("article").click();
    await expect(popover).toBeHidden();
  });

  test("navigates to heading and closes popover on link click", async ({ page }) => {
    await page.goto("/");

    const button = page.getByRole("button", { name: "Table of contents" });
    await button.click();

    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await popover.getByRole("link", { name: "Code Example" }).click();

    // Popover should close after navigation
    await expect(popover).toBeHidden();

    // Target heading should be in viewport
    const heading = page.locator("#code-example");
    await expect(heading).toBeInViewport();
  });

  test("popover is not shown on pages without headings", async ({ page }) => {
    // `installation.md` — the page this used to load — has nine `##` headings,
    // so it always rendered the button. `invoices.md` has none.
    await page.goto("/billing/invoices");

    await expect(page.getByRole("heading", { level: 1 })).toContainText("Invoices");

    // The button renders only when the page has TOC entries, and this page has
    // none. The previous version loaded `installation.md`, which has nine `##`
    // headings, so its `if (count > 0)` guard always ran and asserted the
    // button was *visible* — a real assertion, but the opposite of the case
    // this test is named for. Asserting the absence directly instead.
    await expect(page.getByRole("button", { name: "Table of contents" })).toHaveCount(0);
  });
});
