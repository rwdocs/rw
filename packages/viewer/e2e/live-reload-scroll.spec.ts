import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { type Locator, type Page, test, expect } from "@playwright/test";

const fixture = (path: string) =>
  fileURLToPath(new URL(`./fixtures/docs/${path}`, import.meta.url));
const FIXTURE = fixture("live-scroll.md");
/** 200×100 as committed; a test makes it taller. */
const IMAGE = fixture("live-scroll.svg");

/** The live-reload server; see `playwright.config.ts`. */
const LIVE_URL = "http://127.0.0.1:8084/live-scroll";

/** Two frames: whatever scroll a reload schedules has run by then. */
const scrollYAfterFrames = (page: Page) =>
  page.evaluate(
    () =>
      new Promise<number>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve(window.scrollY))),
      ),
  );

const topOf = async (page: Page, target: Locator) => {
  await scrollYAfterFrames(page);
  return target.evaluate((el) => el.getBoundingClientRect().top);
};

/**
 * Rewrite `file` until `landed` passes. A write made before the viewer's
 * live-reload socket opens sends the viewer nothing, so it is redone.
 */
async function rewriteUntil(file: string, content: string, landed: () => Promise<void>) {
  await expect(async () => {
    await writeFile(file, content, "utf8");
    await landed();
  }).toPass({ timeout: 15000 });
}

/** Rewrite the page until the article shows "Revision 1.". */
async function reloadWith(page: Page, source: string) {
  await rewriteUntil(FIXTURE, source.replace("Revision 0.", "Revision 1."), () =>
    expect(page.getByRole("article")).toContainText("Revision 1.", { timeout: 2000 }),
  );
}

test.describe("Live reload", () => {
  test.describe.configure({ mode: "serial" });

  let original = "";
  let originalImage = "";

  test.beforeAll(async () => {
    original = await readFile(FIXTURE, "utf8");
    originalImage = await readFile(IMAGE, "utf8");
  });

  test.afterEach(async () => {
    await writeFile(FIXTURE, original, "utf8");
    await writeFile(IMAGE, originalImage, "utf8");
  });

  for (const hash of ["", "#section-2"]) {
    test(`keeps the reader's scroll position (URL ${hash || "without a hash"})`, async ({
      page,
    }) => {
      await page.goto(LIVE_URL + hash);
      await expect(page.getByRole("heading", { name: "Section 8" })).toBeAttached();

      await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
      const before = await scrollYAfterFrames(page);
      expect(before).toBeGreaterThan(2000);

      await reloadWith(page, original);

      expect(Math.abs((await scrollYAfterFrames(page)) - before)).toBeLessThanOrEqual(2);
    });
  }

  const cases = [
    {
      name: "a paragraph added above the heading at the top",
      target: "Section 6",
      offset: 0,
      edit: (source: string) => source.replace("## Section 2\n", "## Section 2\n\nAdded.\n"),
    },
    {
      // The heading is out of view, so the paragraph below it, which has no id, anchors.
      name: "a paragraph added above the heading just out of view",
      target: "Section 6",
      offset: -50,
      edit: (source: string) => source.replace("## Section 2\n", "## Section 2\n\nAdded.\n"),
    },
    {
      name: "sentences added to the paragraph across the top",
      target: "Section 5, part 2.",
      offset: -150,
      edit: (source: string) =>
        source.replace("\n\n## Section 6", " Added one. Added two. Added three.\n\n## Section 6"),
    },
  ];
  for (const { name, target, offset, edit } of cases) {
    test(`keeps the text in view in place after ${name}`, async ({ page }) => {
      const block = page.getByRole("article").getByText(target, { exact: false }).first();
      await page.goto(LIVE_URL);
      await expect(block).toBeAttached();
      await block.evaluate((el, offset) => {
        window.scrollBy(0, el.getBoundingClientRect().top - offset);
      }, offset);
      const before = await topOf(page, block);
      expect(Math.abs(before - offset)).toBeLessThanOrEqual(2);

      await reloadWith(page, edit(original));

      expect(Math.abs((await topOf(page, block)) - before)).toBeLessThanOrEqual(2);
    });
  }

  test("keeps the text in view in place when an image above it grows", async ({ page }) => {
    const heading = page.getByRole("heading", { name: "Section 6" });
    const image = page.getByRole("article").getByRole("img", { name: "Resizable" });
    await page.goto(LIVE_URL);
    await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalHeight)).toBe(100);
    await heading.evaluate((el) => window.scrollBy(0, el.getBoundingClientRect().top));
    const before = await topOf(page, heading);

    await writeFile(IMAGE, originalImage.replaceAll('height="100"', 'height="600"'), "utf8");
    await reloadWith(page, original);
    await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalHeight)).toBe(600);

    expect(Math.abs((await topOf(page, heading)) - before)).toBeLessThanOrEqual(2);
  });

  test("shows text added above the title to a reader at the top", async ({ page }) => {
    await page.goto(LIVE_URL);
    await expect(page.getByRole("heading", { name: "Section 8" })).toBeAttached();

    await reloadWith(page, `Added above the title.\n\n${original}`);

    expect(await scrollYAfterFrames(page)).toBe(0);
  });
});
