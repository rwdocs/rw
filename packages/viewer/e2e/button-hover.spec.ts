// Read-only: the comment POST is stubbed, so nothing reaches the shared DB.
import { test, expect, type Locator, type Page } from "@playwright/test";

test.use({ viewport: { width: 1400, height: 800 } });

// Mid-transition colors would let a broken hover pass as unchanged.
async function disableTransitions(page: Page) {
  await page.addStyleTag({ content: "* { transition: none !important; }" });
}

function background(locator: Locator): Promise<string> {
  return locator.evaluate((el) => getComputedStyle(el).backgroundColor);
}

test("a disabled button keeps its fill on hover", async ({ page }) => {
  await page.goto("/");
  await disableTransitions(page);
  await page.getByPlaceholder("Write a comment...").focus();

  const submit = page.getByRole("button", { name: "Comment", exact: true });
  await expect(submit).toHaveAttribute("aria-disabled", "true");
  const resting = await background(submit);

  await submit.hover();
  expect(await background(submit)).toBe(resting);
});

test.describe("dark mode", () => {
  test.use({ colorScheme: "dark" });

  test("the danger button stays filled on hover", async ({ page }) => {
    await page.route("**/_api/comments", async (route) => {
      if (route.request().method() === "POST") {
        await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
      } else {
        await route.fallback();
      }
    });
    await page.goto("/");
    await disableTransitions(page);

    const textarea = page.getByPlaceholder("Write a comment...");
    await textarea.fill("draft");
    await textarea.press("Meta+Enter");

    const retry = page.getByRole("button", { name: "Retry" });
    await expect(retry).toBeVisible();

    await retry.hover();
    expect(await background(retry)).not.toBe("rgba(0, 0, 0, 0)");
  });
});
