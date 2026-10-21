import { rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { type APIRequestContext, test, expect } from "@playwright/test";

const fixture = (path: string) =>
  fileURLToPath(new URL(`./fixtures/docs/${path}`, import.meta.url));
/** Linked from `live-links.md`; missing until the test creates it. */
const CREATED = fixture("live-new.md");
/** Linked from nowhere; missing until the test creates it. */
const UNRELATED = fixture("live-unrelated.md");

/** The live-reload server; see `playwright.config.ts`. */
const LIVE = "http://127.0.0.1:8084";

/**
 * Remove `file`, then wait until the server no longer lists `path`. A page
 * created before the viewer's live-reload socket opens sends the viewer
 * nothing, so a test creates it again; a quick remove and write would reach the
 * server as one edit.
 */
async function removePage(request: APIRequestContext, file: string, path: string) {
  await rm(file, { force: true });
  await expect
    .poll(async () => (await request.get(`${LIVE}/_api/navigation`)).text())
    .not.toContain(`"${path}"`);
}

test("a link to a page created while the page is open starts working", async ({
  page,
  request,
}) => {
  await rm(CREATED, { force: true });
  try {
    await page.goto(`${LIVE}/live-links`);
    const article = page.getByRole("article");
    await expect(article.getByRole("link", { name: "live-new" })).toHaveAttribute("href", "#");

    await expect(async () => {
      await removePage(request, CREATED, "/live-new");
      await writeFile(CREATED, "# Live New\n", "utf8");
      await expect(article.getByRole("link", { name: "Live New" })).toHaveAttribute(
        "href",
        "/live-new",
        { timeout: 2000 },
      );
    }).toPass({ timeout: 15000 });
  } finally {
    await rm(CREATED, { force: true });
  }
});

test("a page created elsewhere leaves the open page's tab and selection alone", async ({
  page,
  request,
}) => {
  await rm(UNRELATED, { force: true });
  try {
    await page.goto(`${LIVE}/tabs`);
    const linux = page.getByRole("tab", { name: "Linux" });
    await linux.click();
    const text = "Install with the shell installer.";

    // Inside the retry too: a parallel spec's image edit rightly replaces the
    // page, clearing the selection.
    await expect(async () => {
      await removePage(request, UNRELATED, "/live-unrelated");
      await page.getByText(text).selectText();
      const refetched = page.waitForResponse(`${LIVE}/_api/pages/tabs`, { timeout: 2000 });
      await writeFile(UNRELATED, "# Live Unrelated\n", "utf8");
      await refetched;
      // Two frames: a replaced page has rendered by then.
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
      );

      await expect(linux).toHaveAttribute("aria-selected", "true", { timeout: 0 });
      expect(await page.evaluate(() => window.getSelection()?.toString())).toBe(text);
    }).toPass({ timeout: 15000 });
  } finally {
    await rm(UNRELATED, { force: true });
  }
});
