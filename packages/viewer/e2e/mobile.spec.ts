import { test, expect } from "@playwright/test";

test.use({ viewport: { width: 390, height: 844 } }); // iPhone 13 dimensions

test.describe("Mobile Navigation", () => {
  test("hides desktop sidebar on mobile", async ({ page }) => {
    await page.goto("/");

    const desktopNav = page.getByRole("complementary", { name: "Sidebar" });
    await expect(desktopNav).toBeHidden();
  });

  test("opens mobile drawer on menu click, with nav items and no rw wordmark", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Open menu" }).click();

    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    await expect(drawer.getByRole("link", { name: "Getting Started" })).toBeVisible();
    await expect(drawer.getByRole("link", { name: "API Reference" })).toBeVisible();

    // ...and no "rw" of its own. Asserted after the links: an absence proves
    // nothing against a subtree that has not rendered. See navigation.spec.ts
    // for why the pattern is anchored and padded.
    await expect(drawer.getByText(/^\s*rw\s*$/i)).toHaveCount(0);
    await expect(drawer.getByRole("img", { name: /^rw$/i })).toHaveCount(0);
  });

  test("table of contents is hidden on mobile", async ({ page }) => {
    await page.goto("/");

    const tocHeading = page.getByRole("heading", {
      name: "On this page",
      exact: true,
      includeHidden: true,
    });
    await expect(tocHeading).toBeHidden();
  });

  test("code blocks are scrollable on mobile", async ({ page }) => {
    await page.goto("/");

    const codeBlock = page.locator("pre").first();
    await expect(codeBlock).toBeVisible();
    await expect(codeBlock).toHaveCSS("overflow-x", "auto");
  });

  test("navigation works across multiple pages on mobile", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });

    const gsLink = drawer.getByRole("link", { name: "Getting Started" });
    await gsLink.locator("..").getByRole("button").click();
    await drawer.getByRole("link", { name: "Installation" }).click();

    await expect(drawer).toBeHidden();
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");

    await page.getByRole("button", { name: "Open menu" }).click();
    await drawer.getByRole("link", { name: "API Reference" }).click();

    await expect(page).toHaveURL(/\/api$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Reference");
  });

  test("breadcrumbs sit in the mobile header and navigate", async ({ page }) => {
    await page.goto("/getting-started/installation");

    const headerBreadcrumbs = page
      .locator("header")
      .getByRole("navigation", { name: "Breadcrumb" });
    await expect(headerBreadcrumbs).toBeVisible();

    // The content-area copy stays mounted under display:none, which role
    // queries skip — a second match means it leaked onto mobile.
    await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveCount(1);

    await headerBreadcrumbs.getByRole("link", { name: "Test Documentation" }).click();
    await expect(page.getByRole("article")).toContainText("Test Documentation");
  });

  test("mobile header TOC button opens popover and navigates to heading", async ({ page }) => {
    await page.goto("/");

    const header = page.locator("header");
    const tocButton = header.getByRole("button", { name: "Table of contents" });
    await expect(tocButton).toBeVisible();
    // Same display:none reasoning as the breadcrumbs above.
    await expect(page.getByRole("button", { name: "Table of contents" })).toHaveCount(1);
    await tocButton.click();

    const popover = page.getByRole("navigation", { name: "Table of contents" });
    await expect(popover).toBeVisible();
    await popover.getByRole("link", { name: "Code Example" }).click();
    await expect(popover).toBeHidden();

    const heading = page.locator("#code-example");
    await expect(heading).toBeInViewport();
    // In the viewport is not enough: the sticky header can still cover it.
    const headerBox = await header.boundingBox();
    const headingBox = await heading.boundingBox();
    expect(headerBox).not.toBeNull();
    expect(headingBox).not.toBeNull();
    expect(headingBox!.y).toBeGreaterThanOrEqual(headerBox!.y + headerBox!.height - 1);
  });

  test("traps Tab within the drawer", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    // Focus lands on the dialog panel itself so screen readers announce it.
    await expect(drawer).toBeFocused();

    // Scoped to the dialog: the backdrop outside it carries the same
    // accessible name.
    const close = drawer.getByRole("button", { name: "Close menu" });
    // The wrap below needs a second focusable, and Close is the only one the
    // drawer always has — wait for the nav so a slow render fails here rather
    // than as a broken trap.
    await expect(drawer.getByRole("link").first()).toBeVisible();

    // First Tab reaches the Close button, confirming Tab enters the drawer at
    // the start of its focusable list rather than escaping.
    await page.keyboard.press("Tab");
    await expect(close).toBeFocused();

    // Shift+Tab from the first control wraps backwards to a different focusable
    // inside the drawer (the last one), so focus leaves the Close button but
    // never leaves the dialog.
    await page.keyboard.press("Shift+Tab");
    await expect(close).not.toBeFocused();
    await expect(drawer.locator(":focus")).toHaveCount(1);
  });

  test("restores focus to the hamburger when closed with Escape", async ({ page }) => {
    await page.goto("/");

    const hamburger = page.getByRole("button", { name: "Open menu" });
    await hamburger.click();

    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(drawer).toBeHidden();
    await expect(hamburger).toBeFocused();
  });

  test("restores focus to the hamburger when closed via the backdrop", async ({ page }) => {
    await page.goto("/");

    const hamburger = page.getByRole("button", { name: "Open menu" });
    await hamburger.click();

    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    // Click the backdrop to the right of the 280px panel.
    const drawerBox = await drawer.boundingBox();
    expect(drawerBox).not.toBeNull();
    await page.mouse.click(drawerBox!.x + drawerBox!.width + 50, drawerBox!.y + 100);

    await expect(drawer).toBeHidden();
    await expect(hamburger).toBeFocused();
  });

  test("inerts the page body behind the open drawer", async ({ page }) => {
    await page.goto("/");

    // A link in the main article is focusable before the drawer opens.
    const articleLink = page.getByRole("article").getByRole("link").first();
    await expect(articleLink).toBeVisible();

    const layoutRoot = page.getByTestId("layout-root");
    await expect(layoutRoot).not.toHaveAttribute("inert", /.*/);

    await page.getByRole("button", { name: "Open menu" }).click();
    await expect(page.getByRole("dialog", { name: "Mobile navigation" })).toBeVisible();

    // While open, the page body carries `inert`, which removes it from the tab
    // order and the a11y tree. Verify the attribute is applied and that focus
    // genuinely cannot enter the body: focusing a link inside it is a no-op
    // because inert elements are not focusable.
    await expect(layoutRoot).toHaveAttribute("inert", /.*/);
    const focusEnteredBody = await articleLink.evaluate((el) => {
      (el as HTMLElement).focus();
      return el.contains(document.activeElement);
    });
    expect(focusEnteredBody).toBe(false);
  });
});
