import { test, expect } from "@playwright/test";

test.describe("Non-ASCII page paths - Embedded", () => {
  test("clicking a nav item navigates and marks it current", async ({ page }) => {
    await page.goto("./гайд");
    const sidebar = page.getByRole("complementary", { name: "Sidebar" });

    await sidebar.getByRole("link", { name: "Шаг" }).click();

    await expect(page.getByRole("article")).toContainText("Первый шаг.");
    await expect(page).toHaveURL(/\/_embedded\/%D0%B3%D0%B0%D0%B9%D0%B4\/%D1%88%D0%B0%D0%B3$/);
    await expect(sidebar.getByRole("link", { name: "Шаг" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  test("clicking a content link navigates the host", async ({ page }) => {
    await page.goto("/гайд");

    await page.getByRole("article").getByRole("link", { name: "К шагу" }).click();

    await expect(page.getByRole("article")).toContainText("Первый шаг.");
    await expect(page).toHaveURL(/\/%D0%B3%D0%B0%D0%B9%D0%B4\/%D1%88%D0%B0%D0%B3$/);
  });
});
