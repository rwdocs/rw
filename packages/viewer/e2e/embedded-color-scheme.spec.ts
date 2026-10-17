import { test, expect } from "@playwright/test";

test.describe("Embedded Color Scheme", () => {
  test("dark mode switches the semantic color tokens", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "light" });
    await page.goto("/diagram");

    const expand = page.getByRole("button", { name: "Expand diagram" });
    const background = () => expand.evaluate((el) => getComputedStyle(el).backgroundColor);
    const light = await background();

    await page.emulateMedia({ colorScheme: "dark" });

    await expect.poll(background).not.toBe(light);
  });
});
