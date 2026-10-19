import { test, expect, type Page } from "@playwright/test";
import { textRect } from "./comment-helpers";

// Wide viewport so the article has empty gutters on both sides.
test.use({ viewport: { width: 1400, height: 800 } });

/**
 * Drag-select with the REAL mouse from (x1,y1) to (x2,y2). Unlike the synthetic
 * `selectText` helper in comment-helpers.ts (which dispatches a mouseup directly on
 * <article>), this fires native events, so the mouseup lands wherever the pointer
 * is released — including outside the article. That is the only way to reproduce
 * the "release in the gutter" bug.
 */
async function dragSelect(page: Page, x1: number, y1: number, x2: number, y2: number) {
  await page.mouse.move(x1, y1);
  await page.mouse.down();
  // Multiple steps emit intermediate mousemove events so the browser registers
  // a drag-selection gesture rather than a single jump.
  await page.mouse.move(x2, y2, { steps: 8 });
  await page.mouse.up();
}

async function articleLeft(page: Page): Promise<number> {
  return page.evaluate(() => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    return article.getBoundingClientRect().left;
  });
}

/**
 * Right-to-left drag-select of the homepage's first word, releasing the mouse
 * just past the article's left edge (outside <article>) — the gutter-release
 * gesture that reproduces the bug. Depends on the homepage fixture
 * (e2e/fixtures/docs/index.md) starting its first paragraph with "Welcome".
 */
async function selectFirstWordReleasingInGutter(page: Page) {
  const word = await textRect(page, "Welcome");
  const aLeft = await articleLeft(page);
  // Guard the premise: at this viewport the article must have a left gutter, or
  // the release point would land inside the article and the test would silently
  // stop exercising the outside-release case.
  expect(aLeft).toBeGreaterThan(20);
  const y = (word.top + word.bottom) / 2;

  // Start at the word's right edge and drag LEFT just past the article's edge
  // into the empty gutter, releasing there (the mouseup target is outside
  // <article>). The 10px offset keeps us in the empty padding, not adjacent
  // sidebar text, so the selection stays within the article (the first word).
  await dragSelect(page, word.right, y, aLeft - 10, y);
}

test.describe("Selection released outside the article", () => {
  test("right-to-left drag of a line's first word still shows the Add comment popover", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectFirstWordReleasingInGutter(page);

    await expect(page.getByRole("button", { name: "Add comment" })).toBeVisible();
  });

  test("clicking the Add comment button after such a selection opens the draft", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectFirstWordReleasingInGutter(page);

    const button = page.getByRole("button", { name: "Add comment" });
    await expect(button).toBeVisible();

    // Real click — exercises the button's mousedown (preventDefault keeps the
    // selection alive) and click. The draft composer opens in the comments sidebar.
    await button.click();
    const sidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(sidebar.getByPlaceholder("Write a comment...")).toBeVisible();
  });
});
