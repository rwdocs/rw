import { readFile } from "node:fs/promises";

import { test, expect, type Page } from "@playwright/test";

function link(page: Page, name: string) {
  return page.getByRole("article").getByRole("link", { name, exact: true });
}

test("a link to a csv downloads the file", async ({ page }) => {
  await page.goto("./files");

  const data = link(page, "data");
  await expect(data).toHaveAttribute("href", "/_api/files/docs/files/data.csv");
  const downloaded = page.waitForEvent("download");
  await data.click();
  const download = await downloaded;

  expect(download.suggestedFilename()).toBe("data.csv");
  expect(await readFile(await download.path(), "utf8")).toBe("name,count\nalpha,1\nbeta,2\n");
});

for (const [name, path, contentType] of [
  ["spec", "/_api/files/docs/files/spec.pdf", "application/pdf"],
  ["img", "/_api/files/docs/images/img/dot.png", "image/png"],
]) {
  test(`a link to ${path.split(".").pop()} opens the file in a new tab`, async ({ page }) => {
    await page.goto("./files");

    const served = page
      .context()
      .waitForEvent("response", (response) => new URL(response.url()).pathname === path);
    const opened = page.waitForEvent("popup");
    await link(page, name).click();
    // Headless Chromium saves a pdf instead of showing it, so the tab's URL
    // proves nothing; the response it got does.
    await opened;
    const response = await served;

    expect(response.status()).toBe(200);
    expect(response.headers()["content-type"]).toBe(contentType);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Files");
  });
}

test("a link to a file rw does not serve stays as written", async ({ page }) => {
  await page.goto("./files");

  const config = link(page, "config");
  await expect(config).toHaveAttribute("href", "config.yaml");
  for (const attribute of ["download", "target"]) {
    await expect(config).not.toHaveAttribute(attribute);
  }
});
