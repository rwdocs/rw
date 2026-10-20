import { test, expect } from "@playwright/test";

// With a host resolver, breadcrumbs and section-root nav items get their hrefs
// from section refs plus page-path subpaths, which must reach the host encoded.
// `reserved/100%` is a section left unmapped, so it resolves through the root.
test.describe("Section-ref hrefs for reserved characters - Embedded", () => {
  const ROOT_BASE = "/_embedded/catalog/default/section/root/docs";

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      const w = window as unknown as {
        __RW_CATALOG_RESOLVER__: boolean;
        __RW_UNMAPPED_REFS__: string[];
      };
      w.__RW_CATALOG_RESOLVER__ = true;
      w.__RW_UNMAPPED_REFS__ = ["system:default/100%"];
    });
    await page.goto(`${ROOT_BASE}/reserved/c%23/intro`);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("C# Intro");
  });

  test("breadcrumb to a page named c# hands the host an encoded path", async ({ page }) => {
    const crumb = page
      .getByRole("navigation", { name: "Breadcrumb" })
      .getByRole("link", { name: "C#" });
    await expect(crumb).toHaveAttribute("href", `${ROOT_BASE}/reserved/c%23`);

    await crumb.click();

    await expect(page).toHaveURL(new RegExp(`${ROOT_BASE}/reserved/c%23$`));
  });

  test("section-root nav item named 100% hands the host an encoded path", async ({ page }) => {
    const item = page
      .getByRole("complementary", { name: "Sidebar" })
      .getByRole("link", { name: "Hundred Percent" });
    await expect(item).toHaveAttribute("href", `${ROOT_BASE}/reserved/100%25`);

    await item.click();

    await expect(page).toHaveURL(new RegExp(`${ROOT_BASE}/reserved/100%25$`));
  });

  // `reserved/sys#1` declares the name `sys1`, so the host maps it like any
  // entity; the link has to be recognized as inside that section.
  test("content link into a section folder named sys#1 goes to that section", async ({ page }) => {
    const link = page.getByRole("article").getByRole("link", { name: "Sys API" });
    await expect(link).toHaveAttribute("href", "/_embedded/catalog/default/system/sys1/docs/api");

    await link.click();

    await expect(page).toHaveURL(/\/_embedded\/catalog\/default\/system\/sys1\/docs\/api$/);
  });

  test("wikilink into a section folder named sys#1 goes to that section", async ({ page }) => {
    const link = page.getByRole("article").getByRole("link", { name: "the sys1 reference" });
    await expect(link).toHaveAttribute("href", "/_embedded/catalog/default/system/sys1/docs/api");

    await link.click();

    await expect(page).toHaveURL(/\/_embedded\/catalog\/default\/system\/sys1\/docs\/api$/);
  });

  test("protocol-relative link stays off site", async ({ page }) => {
    const article = page.getByRole("article");
    // Every section link is rewritten in one pass, so once this one is, the
    // off-site link has been passed over rather than not yet reached.
    await expect(article.getByRole("link", { name: "Sys API" })).toHaveAttribute(
      "href",
      "/_embedded/catalog/default/system/sys1/docs/api",
    );
    await expect(article.getByRole("link", { name: "a protocol-relative link" })).toHaveAttribute(
      "href",
      "//example.com/a",
    );
  });
});
