import { test, expect } from "@playwright/test";

test.describe("Root scope - Standalone", () => {
  test("root page shows its own row and no back link", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toBeVisible();

    // Navigation items should be present
    await expect(sidebar.getByRole("link", { name: "Getting Started" })).toBeVisible();

    // Exactly one link carries the root title: a back link would be a second,
    // uncurrent one.
    const selfRow = sidebar.getByRole("link", { name: "Test Documentation" });
    await expect(selfRow).toHaveCount(1);
    await expect(selfRow).toHaveAttribute("aria-current", "page");

    await expect(sidebar.getByRole("heading", { level: 2 })).toBeHidden();
  });

  test("section page shows back-to-home link", async ({ page }) => {
    // Navigate to a page inside a section (billing/ has kind: domain in meta.yaml)
    await page.goto("/billing/invoices");

    // Wait for page content to load first
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Invoices");

    // The section watcher detects sectionRef and reloads navigation scoped to this section
    const sidebar = page.getByRole("complementary", { name: "Sidebar" });

    // The root tree renders first and already has a "Test Documentation" back
    // link and a "Billing" row of its own, so neither proves the swap
    // happened. A root-only item going away does.
    await expect(sidebar.getByRole("link", { name: "Getting Started" })).toBeHidden();

    const backLink = sidebar.getByRole("link", { name: "Test Documentation" });
    await expect(backLink).toBeVisible();

    // The section's own page is a link now, not a dead heading
    const scopeRow = sidebar.getByRole("link", { name: "Billing" });
    await expect(scopeRow).toBeVisible();
    await expect(sidebar.getByRole("heading", { level: 2 })).toBeHidden();

    // Clicking the back link should navigate to home
    await backLink.click();
    await expect(page.getByRole("article")).toContainText("Test Documentation");
  });
});
