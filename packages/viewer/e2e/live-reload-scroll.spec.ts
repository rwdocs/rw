import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { type Page, test, expect } from "@playwright/test";

const FIXTURE = fileURLToPath(new URL("./fixtures/docs/live-scroll.md", import.meta.url));

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

/**
 * Rewrite the page until the article shows "Revision 1.". A write made before
 * the viewer's live-reload socket opens sends the viewer nothing, so it is redone.
 */
async function reloadWith(page: Page, source: string) {
  await expect(async () => {
    await writeFile(FIXTURE, source.replace("Revision 0.", "Revision 1."), "utf8");
    await expect(page.getByRole("article")).toContainText("Revision 1.", { timeout: 2000 });
  }).toPass({ timeout: 15000 });
}

test.describe("Live reload", () => {
  test.describe.configure({ mode: "serial" });

  for (const hash of ["", "#section-2"]) {
    test(`keeps the reader's scroll position (URL ${hash || "without a hash"})`, async ({
      page,
    }) => {
      const original = await readFile(FIXTURE, "utf8");
      try {
        await page.goto(LIVE_URL + hash);
        await expect(page.getByRole("heading", { name: "Section 8" })).toBeAttached();

        await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
        const before = await scrollYAfterFrames(page);
        expect(before).toBeGreaterThan(2000);

        await reloadWith(page, original);

        expect(Math.abs((await scrollYAfterFrames(page)) - before)).toBeLessThanOrEqual(2);
      } finally {
        await writeFile(FIXTURE, original, "utf8");
      }
    });
  }

  // At 0 the first block in view is the heading; at -50 it is the paragraph
  // below it, which has no id.
  for (const offset of [0, -50]) {
    test(`keeps the text in view in place when a paragraph is added above it (heading at ${offset}px)`, async ({
      page,
    }) => {
      const original = await readFile(FIXTURE, "utf8");
      const heading = page.getByRole("heading", { name: "Section 6" });
      const headingTop = async () => {
        await scrollYAfterFrames(page);
        return heading.evaluate((el) => el.getBoundingClientRect().top);
      };
      try {
        await page.goto(LIVE_URL);
        await expect(heading).toBeAttached();
        await heading.evaluate((el, offset) => {
          window.scrollBy(0, el.getBoundingClientRect().top - offset);
        }, offset);
        const before = await headingTop();
        expect(Math.abs(before - offset)).toBeLessThanOrEqual(2);

        await reloadWith(page, original.replace("## Section 2\n", "## Section 2\n\nAdded.\n"));

        expect(Math.abs((await headingTop()) - before)).toBeLessThanOrEqual(2);
      } finally {
        await writeFile(FIXTURE, original, "utf8");
      }
    });
  }
});
