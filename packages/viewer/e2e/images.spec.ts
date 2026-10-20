import { test, expect } from "@playwright/test";

test("local images load from beside the page and from the project root", async ({ page }) => {
  await page.goto("/images");

  const article = page.getByRole("article");
  for (const name of ["Beside the page", "Spaced name", "From the project root"]) {
    const image = article.getByRole("img", { name });
    await expect(image).toBeVisible();
    await expect
      .poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth), { message: name })
      .toBeGreaterThan(0);
  }
});
