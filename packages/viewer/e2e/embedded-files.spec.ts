import { test, expect } from "@playwright/test";

// A host whose API lives under its own prefix; the route below forwards it to
// `rw serve`'s `/_api`.
const hostScript = `
import { mountRw } from "/lib/embed.js";

mountRw(document.getElementById("rw-root"), {
  apiBaseUrl: "/host/api",
  sectionRef: "section:default/root",
  basePath: "/_embedded",
  location: location.pathname + location.search + location.hash,
});
`;

test("file links lead under the host's apiBaseUrl", async ({ page }) => {
  await page.route("**/_embedded/preview.js", (route) =>
    route.fulfill({ contentType: "text/javascript", body: hostScript }),
  );
  await page.route("**/host/api/**", async (route) => {
    const url = new URL(route.request().url());
    url.pathname = url.pathname.replace("/host/api/", "/_api/");
    await route.fulfill({ response: await route.fetch({ url: url.href }) });
  });

  await page.goto("./files");

  const article = page.getByRole("article");
  await expect(article.getByRole("link", { name: "spec", exact: true })).toHaveAttribute(
    "href",
    "/host/api/files/docs/files/spec.pdf",
  );
  const data = article.getByRole("link", { name: "data", exact: true });
  await expect(data).toHaveAttribute("href", "/host/api/files/docs/files/data.csv");
  const downloaded = page.waitForEvent("download");
  await data.click();
  const download = await downloaded;
  // Playwright saves no routed download, so its bytes can't be read here.
  expect(new URL(download.url()).pathname).toBe("/host/api/files/docs/files/data.csv");
  expect(download.suggestedFilename()).toBe("data.csv");
});
