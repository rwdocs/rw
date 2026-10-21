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
 * Rewrite the page until the article shows `revision`. A write made before the
 * viewer's live-reload socket opens sends the viewer nothing, so it is redone.
 */
async function reloadWith(page: Page, original: string, revision: string) {
  await expect(async () => {
    await writeFile(FIXTURE, original.replace("Revision 0.", revision), "utf8");
    await expect(page.getByRole("article")).toContainText(revision, { timeout: 2000 });
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

        await reloadWith(page, original, "Revision 1.");

        expect(Math.abs((await scrollYAfterFrames(page)) - before)).toBeLessThanOrEqual(2);
      } finally {
        await writeFile(FIXTURE, original, "utf8");
      }
    });
  }
});
