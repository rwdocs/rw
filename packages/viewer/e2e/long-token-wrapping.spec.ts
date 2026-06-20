import { test, expect } from "@playwright/test";

// Regression for issue #387: long URLs and other unbreakable tokens in prose
// paragraphs wrap, while wide tables scroll inside their own .table-wrap box
// (the page itself never overflows).
test.describe("Long-token wrapping", () => {
  // Narrow viewport — any unwrapped URL in the fixture is wider than this.
  test.use({ viewport: { width: 480, height: 720 } });

  test("table cells use overflow-wrap: break-word", async ({ page }) => {
    await page.goto("/wide-content");

    const cell = page
      .getByRole("row", { name: /Webhook/ })
      .getByRole("cell")
      .nth(1);
    await expect(cell).toHaveCSS("overflow-wrap", "break-word");
  });

  test("long URL in table scrolls inside its wrapper, not the page", async ({ page }) => {
    await page.goto("/wide-content");

    // The renderer wraps every table in an accessible horizontal-scroll box.
    const wrapper = page.getByRole("group", { name: "Table" });
    await expect(wrapper).toBeVisible();

    // The unbreakable ~190-char URL makes the table wider than its box, so the
    // wrapper scrolls horizontally instead of the cell wrapping to dozens of
    // 4-character lines.
    const scrolls = await wrapper.evaluate((el) => el.scrollWidth > el.clientWidth);
    expect(scrolls).toBe(true);

    // The page itself must still not overflow horizontally (issue #387).
    const noPageOverflow = await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    );
    expect(noPageOverflow).toBe(true);
  });

  test("table scroll wrapper is keyboard-focusable", async ({ page }) => {
    await page.goto("/wide-content");

    const wrapper = page.getByRole("group", { name: "Table" });
    await wrapper.focus();
    await expect(wrapper).toBeFocused();
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

  test("paragraphs use overflow-wrap: break-word", async ({ page }) => {
    await page.goto("/wide-content");

    const paragraph = page.getByRole("article").locator("p", { hasText: /^Reference:/ });
    await expect(paragraph).toHaveCSS("overflow-wrap", "break-word");
  });
});
