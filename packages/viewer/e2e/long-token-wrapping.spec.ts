import { test, expect } from "@playwright/test";

// Regression for issue #387: long URLs and other unbreakable tokens in prose
// content must wrap instead of forcing horizontal scrolling.
test.describe("Long-token wrapping", () => {
  // Narrow viewport — any unwrapped URL in the fixture is wider than this.
  test.use({ viewport: { width: 480, height: 720 } });

  test("long URL in table cell wraps across multiple lines", async ({ page }) => {
    await page.goto("/wide-content");

    // The row whose first cell is "Webhook" carries the long URL in its second cell.
    const urlCell = page
      .getByRole("row", { name: /Webhook/ })
      .getByRole("cell")
      .nth(1);
    await expect(urlCell).toBeVisible();

    const { height, lineHeight } = await urlCell.evaluate((el) => {
      const cs = getComputedStyle(el);
      return {
        height: el.getBoundingClientRect().height,
        lineHeight: parseFloat(cs.lineHeight),
      };
    });
    // The fixture URL is ~190 chars; at 480px viewport it must wrap to many lines.
    // Without the fix the cell stays single-line (and forces horizontal overflow).
    expect(height).toBeGreaterThan(lineHeight * 3);
  });

  test("long URL in paragraph wraps across multiple lines", async ({ page }) => {
    await page.goto("/wide-content");

    const para = page.getByRole("article").locator("p", { hasText: /^Reference:/ });
    await expect(para).toBeVisible();

    const { height, lineHeight } = await para.evaluate((el) => {
      const cs = getComputedStyle(el);
      return {
        height: el.getBoundingClientRect().height,
        lineHeight: parseFloat(cs.lineHeight),
      };
    });
    expect(height).toBeGreaterThan(lineHeight * 3);
  });

  test("table cells use overflow-wrap: anywhere", async ({ page }) => {
    await page.goto("/wide-content");

    const cell = page
      .getByRole("row", { name: /Webhook/ })
      .getByRole("cell")
      .nth(1);
    await expect(cell).toHaveCSS("overflow-wrap", "anywhere");
  });

  test("paragraphs use overflow-wrap: break-word", async ({ page }) => {
    await page.goto("/wide-content");

    const paragraph = page.getByRole("article").locator("p", { hasText: /^Reference:/ });
    await expect(paragraph).toHaveCSS("overflow-wrap", "break-word");
  });
});
