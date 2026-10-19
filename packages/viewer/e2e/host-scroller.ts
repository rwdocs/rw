import type { Locator, Page } from "@playwright/test";

/**
 * Hand scrolling from the window to the preview shell's content pane, as
 * Backstage does. Returns the pane, which has no role to locate it by.
 */
export async function hostOwnsScroller(page: Page): Promise<Locator> {
  await page.addStyleTag({
    content: `
      html, body { height: 100%; overflow: hidden; }
      #rw-root { height: calc(100vh - 64px); overflow-y: auto; }
    `,
  });
  return page.locator("#rw-root");
}
