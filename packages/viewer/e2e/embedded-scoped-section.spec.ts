import { test, expect } from "@playwright/test";

const BASE = "/_embedded/catalog/default/system/pay/docs";

// The preview shell mounts the site root. This host mounts the `pay` section
// under its catalog URL, as Backstage does.
const hostScript = `
import { mountRw } from "/lib/embed.js";

const hostLocation = () => location.pathname + location.search + location.hash;
const rw = mountRw(document.getElementById("rw-root"), {
  apiBaseUrl: "/_api",
  sectionRef: "system:default/pay",
  basePath: ${JSON.stringify(BASE)},
  location: hostLocation(),
  resolveSectionRefs: async (refs) =>
    Object.fromEntries(
      refs.map((ref) => {
        const [kind, rest] = ref.split(":");
        const [namespace, name] = rest.split("/");
        return [ref, "/_embedded/catalog/" + namespace + "/" + kind + "/" + name + "/docs"];
      }),
    ),
  onNavigate: (href) => history.pushState({}, "", href),
});
addEventListener("popstate", () => rw.navigateTo(hostLocation()));
`;

test.describe("Scoped section - Embedded", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/_embedded/preview.js", (route) =>
      route.fulfill({ contentType: "text/javascript", body: hostScript }),
    );
  });

  test("a link to the scope root with a hash lands on the scope root", async ({ page }) => {
    await page.goto(`${BASE}/config`);
    const heading = page.getByRole("heading", { level: 1 });
    await expect(heading).toContainText("Pay Config");

    await page.getByRole("link", { name: "pay summary" }).click();

    await expect(page).toHaveURL(new RegExp(`${BASE}/#pay$`));
    await expect(heading).toHaveText("Pay");

    await page.reload();
    await expect(heading).toHaveText("Pay");
  });

  // The host pushes on every `onNavigate`, so this also fails if `navigateTo`
  // calls it: Back would push and drop the Forward entry.
  test("Back and Forward hand navigateTo a path inside the section", async ({ page }) => {
    await page.goto(`${BASE}/config`);
    const heading = page.getByRole("heading", { level: 1 });
    await expect(heading).toContainText("Pay Config");
    await page.getByRole("link", { name: "pay summary" }).click();
    await expect(heading).toHaveText("Pay");

    await page.goBack();
    await expect(heading).toContainText("Pay Config");

    await page.goForward();
    await expect(heading).toHaveText("Pay");
  });
});

const BARE_BASE = "/_embedded/docs";

// The same section, from a host that maps no section refs.
const bareHostScript = `
import { mountRw } from "/lib/embed.js";

mountRw(document.getElementById("rw-root"), {
  apiBaseUrl: "/_api",
  sectionRef: "system:default/pay",
  basePath: ${JSON.stringify(BARE_BASE)},
  location: location.pathname + location.search + location.hash,
  onNavigate: (href) => { window.__navigatedTo = href; },
});
`;

test.describe("Scoped section without a resolver - Embedded", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/_embedded/preview.js", (route) =>
      route.fulfill({ contentType: "text/javascript", body: bareHostScript }),
    );
  });

  test("a link outside the section goes to the host as a site path", async ({ page }) => {
    await page.goto(`${BARE_BASE}/config`);
    const article = page.getByRole("article");
    await expect(article.getByRole("link", { name: "pay overview" })).toHaveAttribute(
      "href",
      `${BARE_BASE}/`,
    );

    await article.getByRole("link", { name: "billing invoices" }).click();

    await expect
      .poll(() => page.evaluate(() => (window as { __navigatedTo?: string }).__navigatedTo))
      .toBe("/billing/invoices");
  });
});
