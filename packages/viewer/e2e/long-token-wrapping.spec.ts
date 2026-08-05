import { test, expect, type Locator, type Page } from "@playwright/test";

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

const SECTION = "Long token nav UnbreakableSectionIdentifierOfConsiderableLength section";
const LEAF = "Leaf UnbreakableLeafIdentifierOfConsiderableLength entry";
const SCOPE_PARENT = "Scope UnbreakableCatalogueIdentifierOfConsiderableLength root";
const SCOPE_TITLE = "Inner UnbreakableQueueIdentifierOfConsiderableLength section";
const OUTLINE_ENTRY =
  "Section UnbreakableConfigurationProviderIdentifierOfConsiderableLength details";
// `getByText` matches the DOM text, and the uppercasing is CSS. Pluralized from
// the long-scope fixture's meta.yaml `kind`.
const GROUP_HEADING = "unbreakablecatalogueregistryofconsiderablelengths";

// Gutter (22px) plus the link's own p-1.5, so a label's text starts here
// relative to its row whatever its depth.
const LABEL_INSET = 28;

interface TextLine {
  top: number;
  left: number;
  right: number;
  height: number;
}

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

async function box(target: Locator): Promise<Box> {
  const measured = await target.boundingBox();
  expect(measured, "expected the element to be laid out").not.toBeNull();
  return measured as Box;
}

/**
 * Client rects of the rendered text inside `target`, one per line box. Ranges
 * over text nodes, not the element: a range across an element also yields its
 * children's border boxes, which hide the overflow being measured.
 */
function textLines(target: Locator): Promise<TextLine[]> {
  return target.evaluate((el) => {
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    const rects: DOMRect[] = [];
    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      if (node.nodeValue?.trim() === "") continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      rects.push(...range.getClientRects());
    }
    return rects.map((r) => ({ top: r.top, left: r.left, right: r.right, height: r.height }));
  });
}

async function firstLineMiddle(target: Locator): Promise<number> {
  const lines = await textLines(target);
  expect(lines.length, "expected rendered text to measure").toBeGreaterThan(0);
  return lines[0].top + lines[0].height / 2;
}

/** Right edge of `target`'s content box, excluding padding and any scrollbar. */
function contentRight(target: Locator): Promise<number> {
  return target.evaluate((el) => {
    const { left } = el.getBoundingClientRect();
    return left + el.clientLeft + el.clientWidth - parseFloat(getComputedStyle(el).paddingRight);
  });
}

/**
 * How far the widest line of any `items` match reaches past `column`'s content
 * edge. Pass the element that establishes the column: an ancestor's border box
 * lies outside it and hands the difference over as free overflow.
 */
async function widestOverflow(column: Locator, items: Locator): Promise<number> {
  const edge = await contentRight(column);

  const targets = await items.all();
  expect(targets.length, "expected labels to measure").toBeGreaterThan(0);

  const lines = (await Promise.all(targets.map(textLines))).flat();
  expect(lines.length, "expected rendered text to measure").toBeGreaterThan(0);

  return Math.max(...lines.map((l) => l.right)) - edge;
}

/** Navigate and let webfonts settle: they load with `font-display: swap`, so a
 * late swap reflows the labels between two measurements. */
async function open(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await page.evaluate(() => document.fonts.ready);
}

// The alignment tests live here because a long token is what collapses the
// chevron gutter that keeps a label level with its siblings.
test.describe("Long-token wrapping in the navigation sidebar", () => {
  // The desktop sidebar only renders at container width >= 952px.
  test.use({ viewport: { width: 1280, height: 900 } });

  // The nav landmark is the column itself; the sidebar around it adds px-4.
  const column = (page: Page) => page.getByRole("navigation", { name: "Documentation" });

  // Opening a page inside the section expands the tree to it, so its children
  // are on screen without a click.
  const INSIDE_SECTION = "/long-token/leaf";

  test("a long-token label keeps its indent", async ({ page }) => {
    await open(page, INSIDE_SECTION);
    const nav = column(page);
    const long = nav.getByRole("link", { name: LEAF });
    await expect(long).toBeVisible();

    // Against a sibling, so a wrapped label still lines up with the list...
    const plain = await box(nav.getByRole("link", { name: "Plain sibling" }));
    expect((await box(long)).x).toBeCloseTo(plain.x, 0);

    // ...and against its own row, since a gutter that shrank on every row would
    // keep them level with each other while losing the indent.
    const [firstLine] = await textLines(long);
    expect(firstLine.left - (await box(long.locator(".."))).x).toBeCloseTo(LABEL_INSET, 0);
  });

  test("no nav label extends past its column", async ({ page }) => {
    await open(page, INSIDE_SECTION);
    const nav = column(page);
    // Locator.all() never waits, so hold until the tree is on screen.
    await expect(nav.getByRole("link", { name: LEAF })).toBeVisible();

    expect(await widestOverflow(nav, nav.getByRole("link"))).toBeLessThanOrEqual(0);
  });

  test("a group heading built from a long kind stays inside the column", async ({ page }) => {
    await open(page, INSIDE_SECTION);
    const nav = column(page);
    // A group heading's length is the author's to choose, and it carries no
    // role of its own.
    const heading = nav.getByText(GROUP_HEADING);
    await expect(heading).toBeVisible();

    expect(await widestOverflow(nav, heading)).toBeLessThanOrEqual(0);
  });

  test("the expand chevron sits on the label's first line", async ({ page }) => {
    await open(page, INSIDE_SECTION);
    const link = column(page).getByRole("link", { name: SECTION });

    // A single-line label would satisfy any alignment.
    expect((await textLines(link)).length).toBeGreaterThan(1);

    const chevron = await box(link.locator("..").getByRole("button"));
    expect(chevron.y + chevron.height / 2).toBeCloseTo(await firstLineMiddle(link), 0);
  });

  // A scoped sidebar swaps the tree's top for a back-link to the parent section
  // and a heading for the current one, both rendered from document titles.
  test.describe("scoped to a section", () => {
    test.beforeEach(async ({ page }) => {
      await open(page, "/long-scope/inner");
    });

    // The viewer first renders the root tree and only swaps in the scoped one
    // once the page's section is known. Nothing below auto-waits, so hold here.
    async function scopedNav(page: Page): Promise<Locator> {
      const nav = column(page);
      await expect(nav.getByRole("link", { name: SCOPE_PARENT })).toBeVisible();
      await expect(nav.getByRole("heading", { name: SCOPE_TITLE })).toBeVisible();
      return nav;
    }

    test("the back-link stays inside the column", async ({ page }) => {
      const nav = await scopedNav(page);

      expect(await widestOverflow(nav, nav.getByRole("link"))).toBeLessThanOrEqual(0);
    });

    test("the heading stays inside the column", async ({ page }) => {
      const nav = await scopedNav(page);

      expect(await widestOverflow(nav, nav.getByRole("heading"))).toBeLessThanOrEqual(0);
    });

    test("the back-link label lines up with the heading below it", async ({ page }) => {
      const nav = await scopedNav(page);
      // Measure the label, not the anchor, which starts at the chevron; and
      // text, not boxes, since the heading indents with padding.
      const label = nav.getByRole("link", { name: SCOPE_PARENT }).getByText(SCOPE_PARENT);
      const heading = nav.getByRole("heading", { name: SCOPE_TITLE });

      const [labelLines, headingLines] = await Promise.all([textLines(label), textLines(heading)]);
      expect(labelLines.length).toBeGreaterThan(0);
      expect(headingLines.length).toBeGreaterThan(0);
      expect(labelLines[0].left).toBeCloseTo(headingLines[0].left, 0);
    });

    test("the back-link chevron sits on its label's first line", async ({ page }) => {
      const link = (await scopedNav(page)).getByRole("link", { name: SCOPE_PARENT });
      const label = link.getByText(SCOPE_PARENT);

      expect((await textLines(label)).length).toBeGreaterThan(1);

      const chevron = await box(link.getByRole("img"));
      expect(chevron.y + chevron.height / 2).toBeCloseTo(await firstLineMiddle(label), 0);
    });
  });
});

test.describe("Long-token wrapping in the page outline", () => {
  // The outline column only renders at container width >= 1304px.
  test.use({ viewport: { width: 1440, height: 900 } });

  test("outline entries stay inside the outline column", async ({ page }) => {
    await open(page, "/long-token/leaf");
    const outline = page.getByRole("complementary", { name: "Page outline" });
    await expect(outline.getByRole("link", { name: OUTLINE_ENTRY })).toBeVisible();

    // Measure against the scrolling wrapper, not the aside: its own scrollbar
    // insets the content edge on a long outline.
    const column = page.getByTestId("toc-sticky-wrapper");
    expect(await widestOverflow(column, outline.getByRole("link"))).toBeLessThanOrEqual(0);
  });
});
