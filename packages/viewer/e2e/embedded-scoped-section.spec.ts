import { test, expect } from "@playwright/test";

const BASE = "/catalog/default/system/pay/docs";

// The preview shell mounts without a sectionRef, so its scope is always the
// site root. This host mounts the `pay` section under its catalog URL, as
// Backstage does.
const hostScript = `
import { mountRw } from "/lib/embed.js";

const base = ${JSON.stringify(BASE)};
mountRw(document.getElementById("rw-root"), {
  apiBaseUrl: "/_api",
  sectionRef: "system:default/pay",
  initialPath: (location.pathname.slice(base.length) || "/") + location.hash,
  resolveSectionRefs: async (refs) =>
    Object.fromEntries(
      refs.map((ref) => {
        const [kind, rest] = ref.split(":");
        const [namespace, name] = rest.split("/");
        return [ref, "/catalog/" + namespace + "/" + kind + "/" + name + "/docs"];
      }),
    ),
  onNavigate: (href) => history.pushState({}, "", href),
});
`;

test.describe("Scoped section - Embedded", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/__embedded_preview.js", (route) =>
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
});
