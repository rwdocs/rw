import { test, expect } from "@playwright/test";

test.describe("Page names with URL delimiters - Embedded", () => {
  test("clicking a nav item navigates the host and marks it current", async ({ page }) => {
    await page.goto("./symbols");
    const sidebar = page.getByRole("complementary", { name: "Sidebar" });

    await sidebar.getByRole("link", { name: "C#", exact: true }).click();

    await expect(page.getByRole("article")).toContainText("The C# page.");
    await expect(page).toHaveURL(/\/_embedded\/symbols\/c%23$/);
    await expect(sidebar.getByRole("link", { name: "C#", exact: true })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });
});
