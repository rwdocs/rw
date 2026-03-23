import { test, expect } from "@playwright/test";

test.describe("Root scope - Embedded", () => {
  test("root page has no back link or section title", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toBeVisible();

    // Navigation items should be present
    await expect(sidebar.getByRole("link", { name: "Getting Started" })).toBeVisible();

    // No back link should be rendered (parentScope is null at root)
    await expect(sidebar.getByRole("link", { name: "Test Documentation" })).toBeHidden();

    // No section title heading should be rendered
    await expect(sidebar.getByRole("heading", { level: 2 })).toBeHidden();
  });

  test("section page shows back-to-home link", async ({ page }) => {
    // Navigate to a page inside a section (billing/ has kind: domain in meta.yaml)
    await page.goto("/billing/invoices");

    // Wait for page content to load first
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Invoices");

    // The section watcher detects sectionRef and reloads navigation scoped to this section
    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    const backLink = sidebar.getByRole("link", { name: "Test Documentation" });
    await expect(backLink).toBeVisible();

    // Section title should be displayed
    const sectionTitle = sidebar.getByRole("heading", { level: 2 });
    await expect(sectionTitle).toBeVisible();
    await expect(sectionTitle).toHaveText("Billing");
  });
});
