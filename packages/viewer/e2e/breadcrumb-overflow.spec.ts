import { test, expect, type Page, type Locator } from "@playwright/test";

test.describe("Breadcrumb Overflow", () => {
  // Deep page: 4 ancestor breadcrumbs (Home > Advanced Topics > Plugin Development >
  // Custom Extensions Development); the current-page segment renders as the H1, not the nav.
  const deepPage = "/advanced/plugins/custom-extensions/getting-started-guide";

  async function openEllipsisDropdown(page: Page): Promise<{
    breadcrumbs: Locator;
    ellipsisButton: Locator;
  }> {
    await page.setViewportSize({ width: 400, height: 800 });
    await page.goto(deepPage);

    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    const ellipsisButton = breadcrumbs.getByRole("button", {
      name: "Show hidden breadcrumbs",
    });
    await expect(ellipsisButton).toBeVisible();
    await ellipsisButton.click();

    return { breadcrumbs, ellipsisButton };
  }

  test("collapses middle breadcrumbs into ellipsis when container is narrow", async ({ page }) => {
    // Start wide so all breadcrumbs are visible
    await page.goto(deepPage);

    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(breadcrumbs).toBeVisible();
    await expect(breadcrumbs).toMatchAriaSnapshot(`
      - navigation "Breadcrumb":
        - list:
          - listitem:
            - link "Home":
              - /url: /
            - text: /
          - listitem:
            - link "Advanced Topics":
              - /url: /advanced
            - text: /
          - listitem:
            - link "Plugin Development":
              - /url: /advanced/plugins
            - text: /
          - listitem:
            - link "Custom Extensions Development":
              - /url: /advanced/plugins/custom-extensions
    `);
    // Aria snapshots don't assert absence of unlisted nodes; pin it explicitly.
    await expect(breadcrumbs.getByRole("button", { name: "Show hidden breadcrumbs" })).toBeHidden();

    // Shrink viewport to trigger overflow
    await page.setViewportSize({ width: 400, height: 800 });

    // Ellipsis button + first/last breadcrumbs remain visible; middle items collapse
    await expect(breadcrumbs).toMatchAriaSnapshot(`
      - navigation "Breadcrumb":
        - list:
          - listitem:
            - link "Home":
              - /url: /
            - text: /
          - listitem:
            - button "Show hidden breadcrumbs": …
            - text: /
          - listitem:
            - link "Custom Extensions Development":
              - /url: /advanced/plugins/custom-extensions
    `);
  });

  test("opens dropdown with hidden breadcrumbs on ellipsis click", async ({ page }) => {
    await openEllipsisDropdown(page);

    const dropdown = page.getByRole("menu", { name: "Hidden breadcrumbs" });
    await expect(dropdown).toMatchAriaSnapshot(`
      - menu "Hidden breadcrumbs":
        - menuitem "Advanced Topics"
        - menuitem "Plugin Development"
    `);
  });

  test("navigates when clicking a hidden breadcrumb in dropdown", async ({ page }) => {
    await openEllipsisDropdown(page);

    // Click the hidden breadcrumb
    const dropdown = page.getByRole("menu", { name: "Hidden breadcrumbs" });
    await dropdown.getByRole("menuitem", { name: "Advanced Topics" }).click();

    await expect(page).toHaveURL(/\/advanced$/);
  });

  test("closes dropdown on Escape key", async ({ page }) => {
    const { ellipsisButton } = await openEllipsisDropdown(page);
    await expect(ellipsisButton).toHaveAttribute("aria-expanded", "true");

    await page.keyboard.press("Escape");

    await expect(ellipsisButton).toHaveAttribute("aria-expanded", "false");
  });

  test("closes dropdown on click outside", async ({ page }) => {
    const { ellipsisButton } = await openEllipsisDropdown(page);
    await expect(ellipsisButton).toHaveAttribute("aria-expanded", "true");

    // Click outside
    await page.getByRole("article").click();

    await expect(ellipsisButton).toHaveAttribute("aria-expanded", "false");
  });

  test("shows all breadcrumbs without ellipsis when they fit", async ({ page }) => {
    // 3 short breadcrumbs on a wide viewport should all fit
    await page.goto("/advanced/plugins/custom");

    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(breadcrumbs).toBeVisible();
    await expect(breadcrumbs).toMatchAriaSnapshot(`
      - navigation "Breadcrumb":
        - list:
          - listitem:
            - link "Home":
              - /url: /
            - text: /
          - listitem:
            - link "Advanced Topics":
              - /url: /advanced
            - text: /
          - listitem:
            - link "Plugin Development":
              - /url: /advanced/plugins
    `);
    await expect(breadcrumbs.getByRole("button", { name: "Show hidden breadcrumbs" })).toBeHidden();
  });
});
