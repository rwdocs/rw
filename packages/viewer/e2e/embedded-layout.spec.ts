import { test, expect } from "@playwright/test";
import { hostOwnsScroller } from "./host-scroller";

test.describe("Embedded Layout", () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test("content area does not have internal scroll", async ({ page }) => {
    await page.goto("/");

    const scrollArea = page.getByTestId("content-area");
    await expect(scrollArea).toHaveCSS("overflow-y", "visible");
  });

  test("sidebar is sticky", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toBeVisible();
    await expect(sidebar).toHaveCSS("position", "sticky");
  });

  test("viewer does not overflow its container horizontally", async ({ page }) => {
    await page.goto("/wide-content");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Wide Content");

    const viewer = page.getByTestId("viewer-root");
    const overflow = await viewer.evaluate((el) => ({
      offsetWidth: (el as HTMLElement).offsetWidth,
      scrollWidth: el.scrollWidth,
    }));
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.offsetWidth);
  });

  test("shell header scrolls away when scrolling down", async ({ page }) => {
    await page.goto("/");

    const shellHeader = page.getByRole("banner");
    await expect(shellHeader).toBeInViewport();

    await page.evaluate(() => window.scrollBy(0, 300));

    await expect(shellHeader).not.toBeInViewport();
  });

  test("navigating to a new page scrolls window to top", async ({ page }) => {
    await page.goto("/");

    await page.evaluate(() => window.scrollBy(0, 300));
    const scrolledY = await page.evaluate(() => window.scrollY);
    expect(scrolledY).toBeGreaterThan(0);

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    const gsLink = sidebar.getByRole("link", { name: "Getting Started" });
    const expandButton = gsLink.locator("..").getByRole("button");
    await expandButton.click();
    await sidebar.getByRole("link", { name: "Installation" }).click();

    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");

    const finalY = await page.evaluate(() => window.scrollY);
    expect(finalY).toBe(0);
  });

  test("navigating to a new page scrolls a host-owned pane to top", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();
    const pane = await hostOwnsScroller(page);
    await pane.evaluate((el) => el.scrollBy(0, 300));
    expect(await pane.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await sidebar.getByRole("link", { name: "API Reference" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Reference");

    await expect.poll(() => pane.evaluate((el) => el.scrollTop)).toBe(0);
  });

  test("navigating to a new page scrolls to top when body's overflow scrolls the window", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();
    await page.addStyleTag({
      content: "html, body { height: 100%; } body { overflow-y: auto; }",
    });
    await page.evaluate(() => window.scrollBy(0, 300));
    expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0);

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await sidebar.getByRole("link", { name: "API Reference" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Reference");

    await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
  });

  test("navigating to a new page's heading scrolls a host-owned pane to it", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();
    const pane = await hostOwnsScroller(page);
    await pane.evaluate((el) => el.scrollBy(0, 300));

    // The preview shell hands Back/Forward to `navigateTo`.
    await page.evaluate(() => {
      history.pushState({}, "", "/getting-started/installation#environment-variables");
      dispatchEvent(new PopStateEvent("popstate"));
    });
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");

    const heading = page.getByRole("heading", { name: "Environment Variables" });
    await expect(heading).toBeInViewport();
    expect(await pane.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  });

  test("navigation sidebar shows items and expands", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await expect(sidebar).toBeVisible();
    await expect(sidebar.getByRole("link", { name: "Getting Started" })).toBeVisible();

    const gsLink = sidebar.getByRole("link", { name: "Getting Started" });
    await gsLink.locator("..").getByRole("button").click();
    await expect(sidebar.getByRole("link", { name: "Installation" })).toBeVisible();
  });

  test("page content renders correctly", async ({ page }) => {
    await page.goto("/");

    const article = page.getByRole("article");
    await expect(article).toBeVisible();
    await expect(article).toContainText("Test Documentation");
  });

  test("breadcrumbs display on inner pages", async ({ page }) => {
    await page.goto("/getting-started/installation");

    const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(breadcrumbs).toBeVisible();
    await expect(breadcrumbs.getByRole("link", { name: "Test Documentation" })).toBeVisible();
  });

  test("navigating between pages works", async ({ page }) => {
    await page.goto("/");

    const sidebar = page.getByRole("complementary", { name: "Sidebar" });
    await sidebar.getByRole("link", { name: "API Reference" }).click();

    await expect(page.getByRole("heading", { level: 1 })).toContainText("API Reference");
  });

  test("table of contents is visible on wide viewport", async ({ page }) => {
    // The TOC shows at container-width >= 1304px. The embedded preview shell
    // has a 250px sidebar, so we need viewport >= 1304 + 250 = 1554px.
    await page.setViewportSize({ width: 1600, height: 800 });
    await page.goto("/");

    const toc = page.getByRole("complementary", { name: "Page outline" });
    await expect(toc).toBeVisible();
    await expect(toc.getByText("On this page")).toBeVisible();
  });

  test("outline click keeps the host's history state", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 800 });
    await page.goto("/");
    // The shape React Router keeps there.
    const hostState = { usr: { from: "host" }, key: "k1", idx: 3 };
    await page.evaluate((state) => history.replaceState(state, ""), hostState);

    const toc = page.getByRole("complementary", { name: "Page outline" });
    await toc.getByRole("link", { name: "Code Example" }).click();

    await expect.poll(() => page.evaluate(() => location.hash)).toBe("#code-example");
    expect(await page.evaluate((): unknown => history.state)).toEqual(hostState);
  });

  test("outline click adds one history entry, so one Back undoes it", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 800 });
    await page.goto("/");
    const before = await page.evaluate(() => history.length);

    const toc = page.getByRole("complementary", { name: "Page outline" });
    await toc.getByRole("link", { name: "Code Example" }).click();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe("#code-example");
    expect(await page.evaluate(() => history.length)).toBe(before + 1);

    await page.goBack();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
  });

  test("in-page anchor links keep the host's history state and scroll on every click", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 400 });
    await page.goto("/");
    const hostState = { usr: { from: "host" }, key: "k1", idx: 3 };
    await page.evaluate((state) => history.replaceState(state, ""), hostState);
    // `#` is what the renderer gives a broken link.
    const article = page.getByRole("article");
    await article.evaluate((el) => {
      for (const href of ["#code-example", "#"]) {
        const anchor = document.createElement("a");
        anchor.href = href;
        anchor.textContent = `to ${href}`;
        el.append(anchor);
      }
    });

    const toHeading = article.getByRole("link", { name: "to #code-example" });
    const heading = page.locator("#code-example");
    await toHeading.click();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe("#code-example");
    expect(await page.evaluate((): unknown => history.state)).toEqual(hostState);
    await expect(heading).toBeInViewport();

    // Same link again after scrolling away. `dispatchEvent`, because `click()`
    // would scroll the link, and maybe the heading, into view first.
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(heading).not.toBeInViewport();
    await toHeading.dispatchEvent("click");
    await expect(heading).toBeInViewport();

    await article.getByRole("link", { name: "to #", exact: true }).click();
    await expect.poll(() => page.evaluate(() => location.href)).toMatch(/#$/);
    expect(await page.evaluate((): unknown => history.state)).toEqual(hostState);
  });

  test("hash fragment scrolls to heading", async ({ page }) => {
    await page.goto("/#code-example");

    const heading = page.locator("#code-example");
    await expect(heading).toBeInViewport();
  });
});

test.describe("Embedded Layout - Mobile", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("mobile drawer covers full viewport height", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    const panel = drawer.getByTestId("mobile-drawer-panel");
    const panelBox = await panel.boundingBox();
    expect(panelBox).not.toBeNull();
    expect(panelBox!.height).toBeGreaterThanOrEqual(800);
  });

  test("mobile drawer stays within viewer container horizontally", async ({ page }) => {
    await page.goto("/");

    const viewer = page.getByTestId("viewer-root");
    const viewerBox = await viewer.boundingBox();
    expect(viewerBox).not.toBeNull();

    await page.getByRole("button", { name: "Open menu" }).click();
    const drawer = page.getByRole("dialog", { name: "Mobile navigation" });
    await expect(drawer).toBeVisible();

    const drawerBox = await drawer.boundingBox();
    expect(drawerBox).not.toBeNull();
    expect(drawerBox!.x).toBeGreaterThanOrEqual(viewerBox!.x);
  });

  test("backdrop stays within viewer container horizontally", async ({ page }) => {
    await page.goto("/");

    const viewer = page.getByTestId("viewer-root");
    const viewerBox = await viewer.boundingBox();
    expect(viewerBox).not.toBeNull();

    await page.getByRole("button", { name: "Open menu" }).click();
    const backdrop = page.getByRole("button", { name: "Close menu" }).first();
    await expect(backdrop).toBeVisible();

    const backdropBox = await backdrop.boundingBox();
    expect(backdropBox).not.toBeNull();
    expect(backdropBox!.x).toBeGreaterThanOrEqual(viewerBox!.x);
  });

  test("hash fragment heading is not hidden behind mobile header", async ({ page }) => {
    await page.goto("/getting-started/installation");

    await expect(page.getByRole("button", { name: "Open menu" })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");
    const heading = page.getByRole("heading", { name: "Requirements" });
    await expect(heading).toBeAttached();

    await heading.evaluate((el) => el.scrollIntoView({ behavior: "auto" }));

    const header = page.getByRole("banner", { name: "Mobile header" });
    const headerBox = await header.boundingBox();
    const headingBox = await heading.boundingBox();
    expect(headerBox).not.toBeNull();
    expect(headingBox).not.toBeNull();
    expect(headingBox!.y).toBeGreaterThanOrEqual(headerBox!.y + headerBox!.height);
  });
});
