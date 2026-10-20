import { test, expect } from "@playwright/test";

test("local images load from beside the page and from the project root", async ({ page }) => {
  await page.goto("/images");

  const article = page.getByRole("article");
  for (const name of ["Beside the page", "Spaced name", "From the project root"]) {
    const image = article.getByRole("img", { name });
    await expect(image).toBeVisible();
    await expect(image).toHaveAttribute("src", /^\/_api\/files\//);
    await expect
      .poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth), { message: name })
      .toBeGreaterThan(0);
  }
});

test("a link into the API opens the file instead of a viewer page", async ({ page }) => {
  await page.goto("/images");

  const navigation = page.waitForResponse((response) => response.request().isNavigationRequest());
  await page.getByRole("article").getByRole("link", { name: "Open the dot" }).click();

  const response = await navigation;
  expect(new URL(response.url()).pathname).toBe("/_api/files/docs/images/img/dot.png");
  expect(response.headers()["content-type"]).toBe("image/png");
});
