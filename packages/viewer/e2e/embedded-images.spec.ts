import { test, expect } from "@playwright/test";

// A host whose API lives under its own prefix; the route below forwards it to
// `rw serve`'s `/_api`.
const hostScript = `
import { mountRw } from "/lib/embed.js";

mountRw(document.getElementById("rw-root"), {
  apiBaseUrl: "/host/api",
  sectionRef: "section:default/root",
  basePath: "",
  location: location.pathname + location.search + location.hash,
});
`;

test("local images load from under the host's apiBaseUrl", async ({ page }) => {
  await page.route("**/__embedded_preview.js", (route) =>
    route.fulfill({ contentType: "text/javascript", body: hostScript }),
  );
  await page.route("**/host/api/**", async (route) => {
    const url = new URL(route.request().url());
    url.pathname = url.pathname.replace("/host/api/", "/_api/");
    await route.fulfill({ response: await route.fetch({ url: url.href }) });
  });
  const requested: string[] = [];
  page.on("request", (request) => requested.push(new URL(request.url()).pathname));

  await page.goto("/images");

  const article = page.getByRole("article");
  for (const name of ["Beside the page", "Spaced name", "From the project root"]) {
    const image = article.getByRole("img", { name });
    await expect(image).toHaveAttribute("src", /^\/host\/api\/files\//);
    await expect
      .poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth), { message: name })
      .toBeGreaterThan(0);
  }
  expect(requested.filter((path) => path.startsWith("/_api/"))).toEqual([]);
});
