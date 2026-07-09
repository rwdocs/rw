import { test, expect } from "@playwright/test";

// Regression coverage for #624: in embedded mode, a link whose deepest section
// is NOT registered in the host catalog must resolve against the nearest
// *mapped* ancestor section (base + remainder), instead of falling back to the
// current entity's base (the wrong/"doubled" path).
//
// Fixture: `billing` (domain) contains a nested `pay` (system). The preview
// shell's resolver maps every section ref EXCEPT those in
// `window.__RW_UNMAPPED_REFS__`. Here we leave `system:default/pay` unmapped,
// simulating a docs section with no corresponding host catalog entity. So any
// link into `pay` must resolve via its anchor chain to the billing domain base.
//
// Why the asserted href is the *correct* one and not just *an* answer:
// resolving a link by its single nearest section ref is insufficient here —
// `pay` is unmapped, so that lookup misses and naively falling back to the
// current entity's base yields a wrong path (`/pay` or `/`). The required
// behavior is to walk the ordered anchor chain past unmapped ancestors until a
// mapped one is found (`pay` -> `billing`), giving `<billing base>/pay`. The
// `/catalog/...` prefix can only come from the host resolver via that walk, so
// these exact-href assertions fail unless the anchor-chain walk is intact.
test.describe("Embedded nested entity - cross-scope link resolution (#624)", () => {
  const BILLING_BASE = "/catalog/default/domain/billing/docs";

  test.beforeEach(async ({ page }) => {
    // Turn on the preview shell's host-catalog resolver, and tell it the `pay`
    // system entity is not registered in the catalog (so links into `pay` must
    // fall through the anchor chain to the mapped billing domain).
    await page.addInitScript(() => {
      const w = window as unknown as {
        __RW_CATALOG_RESOLVER__: boolean;
        __RW_UNMAPPED_REFS__: string[];
      };
      w.__RW_CATALOG_RESOLVER__ = true;
      w.__RW_UNMAPPED_REFS__ = ["system:default/pay"];
    });
    await page.goto("/billing/pay/config");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Pay Config");
  });

  test("breadcrumb into an unmapped section resolves via the mapped ancestor, not the current base", async ({
    page,
  }) => {
    const breadcrumb = page.getByRole("navigation", { name: "Breadcrumb" });
    // "Pay" is the current entity's own (unmapped) section. Its anchor chain
    // falls through to the mapped billing domain: <billing base>/pay.
    const payCrumb = breadcrumb.getByRole("link", { name: "Pay" });
    await expect(payCrumb).toHaveAttribute("href", `${BILLING_BASE}/pay`);
  });

  test("breadcrumb to a mapped ancestor section resolves to that entity's base", async ({
    page,
  }) => {
    const breadcrumb = page.getByRole("navigation", { name: "Breadcrumb" });
    // "Billing" is mapped, so it resolves directly to its own base.
    const billingCrumb = breadcrumb.getByRole("link", { name: "Billing" });
    await expect(billingCrumb).toHaveAttribute("href", BILLING_BASE);
  });

  test("content link into an unmapped section resolves via the mapped ancestor", async ({
    page,
  }) => {
    // config.md links to /billing/pay — deepest section `pay` is unmapped, so
    // it must resolve to the billing base + the in-domain remainder ("pay").
    const link = page.getByRole("link", { name: "pay overview" });
    await expect(link).toHaveAttribute("href", `${BILLING_BASE}/pay`);
  });
});

// Regression coverage for #624: the block above only unmaps `system:default/pay`,
// whose ancestor `billing` is still mapped — so resolution never has to walk
// PAST an intermediate ancestor. This block additionally unmaps
// `domain:default/billing`, forcing the walk two hops up the anchor chain
// (pay -> billing -> root) to the always-mapped site-root entity
// (`section:default/root`), which is the branch the original coverage missed.
test.describe("Embedded nested entity - walk-to-root fallback (#624)", () => {
  // The preview resolver maps `kind:namespace/name` to
  // `/catalog/{namespace}/{kind}/{name}/docs`. The fixture's root scope has no
  // `meta.yaml`, so it defaults to kind `section`, name `root`, namespace
  // `default` -> ref `section:default/root`.
  const ROOT_BASE = "/catalog/default/section/root/docs";

  test.beforeEach(async ({ page }) => {
    // Unmap BOTH `pay` (the link's own section) and `billing` (its nearest
    // ancestor), leaving only the site root mapped. Any link into `pay` must
    // now walk past two unmapped rungs to reach root.
    await page.addInitScript(() => {
      const w = window as unknown as {
        __RW_CATALOG_RESOLVER__: boolean;
        __RW_UNMAPPED_REFS__: string[];
      };
      w.__RW_CATALOG_RESOLVER__ = true;
      w.__RW_UNMAPPED_REFS__ = ["system:default/pay", "domain:default/billing"];
    });
    await page.goto("/billing/pay/config");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Pay Config");
  });

  test("breadcrumb into a doubly-unmapped section resolves via the root ancestor, preserving the full remainder", async ({
    page,
  }) => {
    const breadcrumb = page.getByRole("navigation", { name: "Breadcrumb" });
    // "Pay" is the current entity's own (unmapped) section, and its nearest
    // ancestor `billing` is unmapped too, so the walk continues to root. The
    // root rung's ancestry subpath for a `pay` link is the full path from root
    // down to `pay`, i.e. "billing/pay" -> root base + "/billing/pay".
    const payCrumb = breadcrumb.getByRole("link", { name: "Pay" });
    await expect(payCrumb).toHaveAttribute("href", `${ROOT_BASE}/billing/pay`);
  });

  test("breadcrumb to the intermediate unmapped ancestor also resolves via root", async ({
    page,
  }) => {
    const breadcrumb = page.getByRole("navigation", { name: "Breadcrumb" });
    // "Billing" is unmapped now too, so it also falls through to root. The
    // root rung's ancestry subpath for the `billing` link is "billing" (the
    // path from root down to billing) -> root base + "/billing".
    const billingCrumb = breadcrumb.getByRole("link", { name: "Billing" });
    await expect(billingCrumb).toHaveAttribute("href", `${ROOT_BASE}/billing`);
  });

  test("content link into the doubly-unmapped section resolves via root, past the unmapped intermediate ancestor", async ({
    page,
  }) => {
    // config.md links to /billing/pay — both `pay` and `billing` are unmapped,
    // so resolution must walk two hops to root, giving root base + the full
    // "billing/pay" remainder. If the walk stopped early (or fell back to the
    // current entity's base) this would instead be a wrong/doubled path.
    const link = page.getByRole("link", { name: "pay overview" });
    await expect(link).toHaveAttribute("href", `${ROOT_BASE}/billing/pay`);
  });
});
