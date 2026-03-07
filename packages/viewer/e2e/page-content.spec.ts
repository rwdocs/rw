import { test, expect } from "@playwright/test";

test.describe("ToC sticky behavior", () => {
  test.use({ viewport: { width: 1400, height: 400 } });

  test("ToC stays visible at top when content is scrolled down", async ({ page }) => {
    await page.goto("/");

    // Wait for TOC to be visible
    const tocHeading = page.getByText("On this page");
    await expect(tocHeading).toBeVisible();

    // Scroll the content area down (the overflow-y-auto wrapper inside .layout-root)
    await page.evaluate(() => {
      const scrollContainer = document.querySelector(".layout-root > .overflow-y-auto");
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    });

    // The TOC should still be visible at the top after scrolling
    await expect(tocHeading).toBeInViewport();
  });
});

test.describe("Embedded Layout", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("sidebar height is bounded by container, not viewport", async ({ page }) => {
    await page.goto("/");

    // Wait for desktop sidebar to be visible (requires >= 952px container width)
    const sidebar = page.locator(".layout-sidebar");
    await expect(sidebar).toBeVisible();

    // Simulate embedded mode: constrain the viewer container to be much
    // shorter than the viewport (e.g., host app has a header/footer).
    const containerHeight = 400;
    await page.evaluate((h) => {
      const container = document.querySelector(".layout-container") as HTMLElement;
      container.style.height = `${h}px`;
      container.style.overflow = "hidden";
    }, containerHeight);

    // The sidebar height should not exceed the container height.
    // With the bug, it uses h-screen (100vh = 900px) which overflows
    // the 400px container.
    const sidebarHeight = await sidebar.evaluate((el) => el.getBoundingClientRect().height);

    expect(sidebarHeight).toBeLessThanOrEqual(containerHeight);
  });

  test("ToC max-height is bounded by container, not viewport", async ({ page }) => {
    await page.goto("/");

    // Wait for ToC to be visible (requires >= 1224px container width)
    const tocAside = page.locator(".layout-toc");
    await expect(tocAside).toBeVisible();

    // Simulate embedded mode: constrain the viewer container to be much
    // shorter than the viewport (e.g., host app has a header/footer).
    const containerHeight = 400;
    await page.evaluate((h) => {
      const container = document.querySelector(".layout-container") as HTMLElement;
      container.style.height = `${h}px`;
      container.style.overflow = "hidden";
    }, containerHeight);

    // The ToC sticky wrapper's max-height should not exceed the container
    // height. With the bug, it uses 100vh (~900px) which is much larger
    // than the 400px container.
    await expect(async () => {
      const maxH = await page.evaluate(() => {
        const toc = document.querySelector(".layout-toc .sticky") as HTMLElement;
        return parseFloat(getComputedStyle(toc).maxHeight);
      });
      expect(maxH).toBeLessThanOrEqual(containerHeight);
    }).toPass({ timeout: 2000 });
  });

  test("sidebar navigation is scrollable when container is shorter than content", async ({
    page,
  }) => {
    await page.goto("/");

    const sidebar = page.locator(".layout-sidebar");
    await expect(sidebar).toBeVisible();

    // Expand all navigation sections so the sidebar content is tall
    for (const toggle of await sidebar.locator("button").all()) {
      await toggle.click();
    }

    // Simulate embedded mode: the host app wraps the viewer in a
    // fixed-height container with overflow hidden. The viewer's own
    // sidebar must remain scrollable within these bounds.
    const containerHeight = 150;
    await page.evaluate((h) => {
      const container = document.querySelector(".layout-container") as HTMLElement;
      const wrapper = document.createElement("div");
      wrapper.style.height = `${h}px`;
      wrapper.style.overflow = "hidden";
      container.parentElement!.insertBefore(wrapper, container);
      wrapper.appendChild(container);
    }, containerHeight);

    // The sidebar content should overflow and be scrollable
    await expect(async () => {
      const { isScrollable, canScrollToBottom } = await page.evaluate(() => {
        const sb = document.querySelector(".layout-sidebar") as HTMLElement;
        const overflows = sb.scrollHeight > sb.clientHeight;
        sb.scrollTop = sb.scrollHeight;
        return {
          isScrollable: overflows,
          canScrollToBottom: sb.scrollTop > 0,
        };
      });
      expect(isScrollable).toBe(true);
      expect(canScrollToBottom).toBe(true);
    }).toPass({ timeout: 2000 });
  });
});

test.describe("Page Content", () => {
  test("renders markdown content with headings", async ({ page }) => {
    await page.goto("/");

    const article = page.locator("article");
    await expect(article).toBeVisible();

    // Should have page title (h1)
    await expect(article.locator("h1")).toContainText("Test Documentation");

    // Should have section headings (h2)
    await expect(article.locator("#features")).toBeVisible();
    await expect(article.locator("#quick-start")).toBeVisible();
    await expect(article.locator("#code-example")).toBeVisible();
  });

  test("renders lists correctly", async ({ page }) => {
    await page.goto("/");

    const article = page.locator("article");

    // Should have unordered list with features
    const list = article.locator("ul").first();
    await expect(list).toContainText("Navigation sidebar with expand/collapse");
    await expect(list).toContainText("Markdown rendering with code highlighting");
  });

  test("renders code blocks with syntax highlighting", async ({ page }) => {
    await page.goto("/");

    // Should have code blocks
    const codeBlocks = page.locator("pre code");
    await expect(codeBlocks.first()).toBeVisible();

    // Code block should have language class for highlighting
    await expect(codeBlocks.first()).toHaveAttribute("class", /language-typescript/);

    // Should also have Python code block
    const pythonBlock = page.locator('pre code[class*="language-python"]');
    await expect(pythonBlock).toBeVisible();
    await expect(pythonBlock).toContainText("def greet");
  });

  test("renders internal links correctly", async ({ page }) => {
    await page.goto("/");

    const article = page.locator("article");

    // Should have internal links
    const gettingStartedLink = article.getByRole("link", { name: "Getting Started" });
    await expect(gettingStartedLink).toBeVisible();

    // Click internal link
    await gettingStartedLink.click();

    // Should navigate to the linked page
    await expect(page).toHaveURL(/\/getting-started$/);
    await expect(page.locator("article h1")).toContainText("Getting Started");
  });

  test("renders tables correctly", async ({ page }) => {
    await page.goto("/getting-started/configuration");

    const article = page.locator("article");

    // Should have a table
    const table = article.locator("table").first();
    await expect(table).toBeVisible();

    // Table should have headers
    await expect(table.locator("th")).toContainText(["Variable", "Description", "Default"]);

    // Table should have data rows (use first matching cell)
    await expect(table.getByRole("cell", { name: "HOST", exact: true })).toBeVisible();
    await expect(table.getByRole("cell", { name: "Server host" })).toBeVisible();
  });

  test("renders ordered lists correctly", async ({ page }) => {
    await page.goto("/getting-started/configuration");

    const article = page.locator("article");

    // Should have ordered list
    const orderedList = article.locator("ol").first();
    await expect(orderedList).toBeVisible();
    await expect(orderedList.locator("li")).toHaveCount(3);
    await expect(orderedList).toContainText("Default values");
    await expect(orderedList).toContainText("Configuration file");
    await expect(orderedList).toContainText("Environment variables");
  });

  test("shows table of contents", async ({ page }) => {
    await page.goto("/");

    // ToC should be visible (has "On this page" heading)
    await expect(page.getByText("On this page")).toBeVisible();

    // Should contain heading links
    await expect(page.getByRole("link", { name: "Features" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Quick Start" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Code Example" })).toBeVisible();
  });

  test("table of contents scrolls to section on click", async ({ page }) => {
    await page.goto("/");

    // Click on Code Example in ToC
    await page.getByRole("link", { name: "Code Example" }).click();

    // Code Example heading should be in viewport
    const codeExampleHeading = page.locator("#code-example");
    await expect(codeExampleHeading).toBeInViewport();
  });

  test("shows 404 page for missing content", async ({ page }) => {
    await page.goto("/nonexistent-page");

    // Should show not found message
    await expect(page.locator("body")).toContainText(/not found/i);
  });

  test("handles deeply nested pages", async ({ page }) => {
    await page.goto("/advanced/plugins/custom");

    const article = page.locator("article");
    await expect(article).toBeVisible();

    // Should show correct content
    await expect(article.locator("h1")).toContainText("Custom Plugin Guide");
    await expect(article).toContainText("Step 1: Create Plugin Structure");
    await expect(article).toContainText("Step 2: Register Plugin");
  });

  test("renders multiple code blocks with different languages", async ({ page }) => {
    await page.goto("/advanced/plugins/custom");

    // Should have bash code block
    const bashBlock = page.locator('pre code[class*="language-bash"]');
    await expect(bashBlock.first()).toBeVisible();

    // Should have typescript code block
    const tsBlock = page.locator('pre code[class*="language-typescript"]');
    await expect(tsBlock).toBeVisible();
    await expect(tsBlock).toContainText("my-plugin");

    // Should have toml code block
    const tomlBlock = page.locator('pre code[class*="language-toml"]');
    await expect(tomlBlock).toBeVisible();
  });

  test("internal links with relative paths work", async ({ page }) => {
    await page.goto("/advanced/plugins/custom");

    const article = page.locator("article");

    // Click link to installation (uses ../../getting-started/installation.md)
    const installationLink = article.getByRole("link", { name: "Installation" });
    await expect(installationLink).toBeVisible();
    await installationLink.click();

    // Should navigate correctly
    await expect(page).toHaveURL(/\/getting-started\/installation$/);
  });

  test("page title updates on navigation", async ({ page }) => {
    await page.goto("/");

    // Initial title should be "{page title} - RW"
    await expect(page).toHaveTitle(/Test Documentation - RW/);

    // Navigate to another page
    await page.goto("/getting-started/installation");

    // Title should update
    await expect(page).toHaveTitle(/Installation - RW/);
  });
});
