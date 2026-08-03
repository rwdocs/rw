import { test, expect, type Page } from "@playwright/test";

// 1400px is wide enough for the full TOC sidebar (>=1304px container width).
test.use({ viewport: { width: 1400, height: 800 } });

const outline = (page: Page) => page.getByRole("complementary", { name: "Page outline" });

const entry = (page: Page, name: string) => outline(page).getByRole("link", { name });

/**
 * Only ever asserted alongside a negative `entry(...)` check, which passes when
 * nothing at all is highlighted. An attribute selector because no role-based
 * locator can ask which entry is current without naming it.
 */
const highlighted = (page: Page) => outline(page).locator("a[aria-current='true']");

test.describe("Page outline", () => {
  test("resets to the first entry when switching pages", async ({ page }) => {
    // The two fixtures deliberately share every heading id: that is what used
    // to defeat the reset, since the previous entry was still in the new list.
    await page.goto("/outline/alpha");
    await expect(outline(page)).toBeVisible();

    // Scroll deep into the document so some entry past the first is current.
    await page.mouse.wheel(0, 4000);
    await expect(entry(page, "Overview")).not.toHaveAttribute("aria-current", "true");
    await expect(highlighted(page)).toHaveCount(1);

    await page
      .getByRole("complementary", { name: "Sidebar" })
      .getByRole("link", { name: "Beta Outline Document" })
      .click();

    await expect(page).toHaveURL(/\/outline\/beta$/);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Beta Outline Document");
    // Above the highlight line, so the outline answers "first entry" because the
    // reader has passed no heading — not because a jump landed elsewhere.
    expect(await page.evaluate(() => window.scrollY < window.innerHeight * 0.2)).toBe(true);

    await expect(entry(page, "Overview")).toHaveAttribute("aria-current", "true");
  });

  test("activates the final entry at the bottom of the page", async ({ page }) => {
    await page.goto("/outline/alpha");

    // Re-scroll on each attempt: a late reflow (web font, image) grows the
    // document after the first jump, which would leave the page short of the
    // bottom and the rule under test correctly dormant.
    await expect(async () => {
      await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
      await expect(entry(page, "Outcomes and Retrospective")).toHaveAttribute(
        "aria-current",
        "true",
        { timeout: 1000 },
      );
    }).toPass({ timeout: 10_000 });
  });

  test("keeps a clicked entry current when the click bottoms the page out", async ({ page }) => {
    await page.goto("/outline/alpha");

    // The last section is short, so jumping to the one before it scrolls the
    // document as far as it goes — where geometry alone resolves to the final
    // entry, which is not the one the reader chose.
    await entry(page, "Decision Log").click();
    await expect(page).toHaveURL(/#decision-log$/);
    await expect(entry(page, "Decision Log")).toHaveAttribute("aria-current", "true");

    // The premise: the jump really did bottom the page out. Lengthening the
    // fixture's tail would otherwise make this test pass with no pin at all.
    expect(
      await page.evaluate(
        () => window.scrollY >= document.documentElement.scrollHeight - window.innerHeight - 2,
      ),
    ).toBe(true);

    // Wait out the click's scroll — suppression releases on `scrollend` — then
    // force the recompute a reflow elsewhere on the page would cause. The
    // geometry is deliberately left alone: resizing the viewport would grow the
    // document and lift it off the bottom, making the rule under test dormant
    // and the assertion vacuous.
    await page.evaluate(async () => {
      await new Promise<void>((resolve) => {
        window.addEventListener("scrollend", () => resolve(), { once: true });
        // Not every browser fires `scrollend`; the hook has its own 500ms
        // fallback, so outlast that.
        setTimeout(resolve, 800);
      });
      window.dispatchEvent(new Event("resize"));
    });

    await expect(entry(page, "Decision Log")).toHaveAttribute("aria-current", "true");
  });

  test("keeps a deep-linked entry current when the link bottoms the page out", async ({ page }) => {
    // The scroll to a hash target starts in PageContent a frame after the
    // outline has chosen the entry, so it must not read as the reader
    // scrolling away from what the link asked for.
    await page.goto("/outline/alpha#decision-log");

    await expect(entry(page, "Decision Log")).toHaveAttribute("aria-current", "true");

    // The premise: geometry on its own would not choose this entry, because the
    // page cannot scroll it up to the line. Without that, the assertion above
    // would pass whether or not the link was honoured.
    expect(
      await page.evaluate(() => {
        const heading = document.getElementById("decision-log")!;
        return heading.getBoundingClientRect().top > window.innerHeight * 0.2;
      }),
    ).toBe(true);

    // And it still hands back to geometry once the reader moves.
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(entry(page, "Overview")).toHaveAttribute("aria-current", "true");
  });

  test("follows the reader back to the top of the same document", async ({ page }) => {
    await page.goto("/outline/alpha");
    // Wait for the outline before scrolling: a wheel delivered while the page
    // is still rendering is swallowed, leaving the reader at the top.
    await expect(outline(page)).toBeVisible();

    await page.mouse.wheel(0, 4000);
    await expect(entry(page, "Overview")).not.toHaveAttribute("aria-current", "true");
    await expect(highlighted(page)).toHaveCount(1);

    await page.evaluate(() => window.scrollTo(0, 0));

    await expect(entry(page, "Overview")).toHaveAttribute("aria-current", "true");
  });
});
