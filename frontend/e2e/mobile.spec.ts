import { test, expect } from "@playwright/test";

test.use({ viewport: { width: 390, height: 844 } }); // iPhone 13 dimensions

test.describe("Mobile Navigation", () => {
  test("hides desktop sidebar on mobile", async ({ page }) => {
    await page.goto("/index");

    // Desktop aside (has md:block class meaning visible on desktop, hidden on mobile)
    // On mobile the aside should not be visible
    const desktopNav = page.locator("aside").first();
    await expect(desktopNav).toBeHidden();
  });

  test("shows hamburger menu button", async ({ page }) => {
    await page.goto("/index");

    // Hamburger button should be visible (aria-label="Open menu")
    const menuButton = page.getByRole("button", { name: "Open menu" });
    await expect(menuButton).toBeVisible();
  });

  test("opens mobile drawer on menu click", async ({ page }) => {
    await page.goto("/index");

    // Click hamburger menu
    await page.getByRole("button", { name: "Open menu" }).click();

    // Mobile drawer should appear (it's a fixed aside)
    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Should show navigation items
    await expect(drawer.getByRole("link", { name: "Usage" })).toBeVisible();
  });

  test("closes drawer on navigation", async ({ page }) => {
    await page.goto("/index");

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();

    // Drawer should be open
    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Find expand button next to Usage link and click it
    const usageLink = drawer.getByRole("link", { name: "Usage" });
    const expandButton = usageLink.locator("..").getByRole("button");
    await expandButton.click();

    // Click Server link
    await drawer.getByRole("link", { name: "Server" }).click();

    // Drawer should close after navigation
    await expect(drawer).toBeHidden();

    // Should have navigated
    await expect(page).toHaveURL(/\/usage\/server$/);
  });

  test("closes drawer on escape key", async ({ page }) => {
    await page.goto("/index");

    // Open drawer
    await page.getByRole("button", { name: "Open menu" }).click();

    // Drawer should be open
    const drawer = page.locator("aside.fixed");
    await expect(drawer).toBeVisible();

    // Press escape
    await page.keyboard.press("Escape");

    // Drawer should close
    await expect(drawer).toBeHidden();
  });

  test("content is readable on mobile", async ({ page }) => {
    await page.goto("/index");

    // Article should be visible
    const article = page.locator("article");
    await expect(article).toBeVisible();

    // Content should be readable
    await expect(article).toContainText("Docstage");
    await expect(article).toContainText("Features");
  });
});
