import { copyFile, readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { type Locator, test, expect } from "@playwright/test";

const fixture = (path: string) => fileURLToPath(new URL(`./fixtures/${path}`, import.meta.url));

/** 1×1 as committed; the tests swap in a 3×2 PNG. */
const EDITED = fixture("docs/images/img/live.png");
/** Referenced by the page but missing until a test adds it. */
const ADDED = fixture("docs/images/img/added.png");
const WIDE = fixture("live-image-3x2.png");

/** The live-reload server; see `playwright.config.ts`. */
const LIVE_URL = "http://127.0.0.1:8084/images/live";
/** Shows no image `LIVE_URL` shows. */
const OTHER_URL = "http://127.0.0.1:8084/images";

const naturalWidth = (image: Locator) =>
  image.evaluate((img: HTMLImageElement) => img.naturalWidth);

/**
 * Copy the wide PNG to `target` until `image` shows it. A copy made before the
 * viewer's live-reload socket opens sends the viewer nothing, so it is redone.
 */
async function showWide(image: Locator, target: string) {
  await expect(async () => {
    await copyFile(WIDE, target);
    await expect.poll(() => naturalWidth(image), { timeout: 2000 }).toBe(3);
  }).toPass({ timeout: 15000 });
}

test.describe("Live reload of local images", () => {
  test.describe.configure({ mode: "serial" });

  test("an edited image shows its new content although the page's HTML is unchanged", async ({
    page,
  }) => {
    const original = await readFile(EDITED);
    try {
      await page.goto(LIVE_URL);
      const image = page.getByRole("article").getByRole("img", { name: "Edited image" });
      await expect.poll(() => naturalWidth(image)).toBe(1);

      await showWide(image, EDITED);
    } finally {
      await writeFile(EDITED, original);
    }
  });

  test("an image added after the page loaded shows", async ({ page }) => {
    await rm(ADDED, { force: true });
    try {
      await page.goto(LIVE_URL);
      const image = page.getByRole("article").getByRole("img", { name: "Added image" });
      await expect(image).toBeAttached();
      expect(await naturalWidth(image)).toBe(0);

      await showWide(image, ADDED);
    } finally {
      await rm(ADDED, { force: true });
    }
  });

  test("an image another page shows changing keeps this page's selection", async ({ page }) => {
    const original = await readFile(EDITED);
    try {
      // The server only reports a file some page has been rendered with.
      await page.goto(LIVE_URL);
      await expect(page.getByRole("img", { name: "Edited image" })).toBeAttached();

      await page.goto(OTHER_URL);
      const article = page.getByRole("article");
      await article.getByRole("heading", { name: "Images" }).selectText();
      const selected = () => page.evaluate(() => window.getSelection()!.toString());
      expect(await selected()).toBe("Images");

      let refetches = 0;
      page.on("requestfinished", (request) => {
        if (request.url().endsWith("/_api/pages/images")) refetches++;
      });
      await expect(async () => {
        await copyFile(WIDE, EDITED);
        await expect.poll(() => refetches, { timeout: 2000 }).toBeGreaterThan(0);
      }).toPass({ timeout: 15000 });
      await expect(article.locator('img[src*="rw-files="]').first()).toBeAttached();
      // Two frames: a page the refetch replaces is on screen by then.
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
      );

      expect(await selected()).toBe("Images");
    } finally {
      await writeFile(EDITED, original);
    }
  });
});
