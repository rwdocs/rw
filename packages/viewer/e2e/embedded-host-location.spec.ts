import { test, expect } from "@playwright/test";

// The preview shell hands `mountRw` and `navigateTo` its raw location.
test.describe("Host location - Embedded", () => {
  for (const { url, text } of [
    { url: "./symbols/c%23", text: "The C# page." },
    { url: "./symbols/100%2525", text: "The 100%25 page." },
    { url: "./reserved/100%25", text: "Hundred Percent" },
    { url: "./%D0%B3%D0%B0%D0%B9%D0%B4/%D1%88%D0%B0%D0%B3", text: "Первый шаг." },
    { url: "./symbols/c%23?tab=x#top", text: "The C# page." },
  ]) {
    test(`opens ${url}`, async ({ page }) => {
      await page.goto(url);

      await expect(page.getByRole("article")).toContainText(text);
    });
  }

  test("opens a page under a host basePath", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __RW_CATALOG_RESOLVER__: boolean }).__RW_CATALOG_RESOLVER__ = true;
    });
    await page.goto("./catalog/default/section/root/docs/symbols/100%2525?tab=x");

    await expect(page.getByRole("article")).toContainText("The 100%25 page.");
  });

  test("Back and Forward return to pages named with escapes", async ({ page }) => {
    await page.goto("./symbols/c%23");
    const article = page.getByRole("article");
    await expect(article).toContainText("The C# page.");
    const sidebar = page.getByRole("complementary", { name: "Sidebar" });

    await sidebar.getByRole("link", { name: "100%25", exact: true }).click();
    await expect(article).toContainText("The 100%25 page.");

    await page.goBack();
    await expect(article).toContainText("The C# page.");
    await page.goForward();
    await expect(article).toContainText("The 100%25 page.");
  });
});
