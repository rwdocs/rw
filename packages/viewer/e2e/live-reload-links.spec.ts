import { rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { test, expect } from "@playwright/test";

/** Linked from `live-links.md`; missing until the test creates it. */
const CREATED = fileURLToPath(new URL("./fixtures/docs/live-new.md", import.meta.url));

/** The live-reload server; see `playwright.config.ts`. */
const LIVE = "http://127.0.0.1:8084";

test("a link to a page created while the page is open starts working", async ({
  page,
  request,
}) => {
  await rm(CREATED, { force: true });
  try {
    await page.goto(`${LIVE}/live-links`);
    const article = page.getByRole("article");
    await expect(article.getByRole("link", { name: "live-new" })).toHaveAttribute("href", "#");

    // A page created before the viewer's live-reload socket opens sends the
    // viewer nothing. To create it again, the server must first see it gone:
    // a quick remove and write reaches it as one edit.
    await expect(async () => {
      await rm(CREATED, { force: true });
      await expect
        .poll(async () => (await request.get(`${LIVE}/_api/navigation`)).text())
        .not.toContain('"/live-new"');
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
