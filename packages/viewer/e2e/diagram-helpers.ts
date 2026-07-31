import { type Page } from "@playwright/test";

/**
 * Open the diagram viewer.
 *
 * The expand button is hover-revealed (opacity), so the figure is hovered
 * first — that is the path a user takes, and it keeps the click subject to
 * Playwright's actionability checks rather than forcing it past them.
 */
export async function openDiagram(page: Page) {
  await page
    .locator("figure")
    .filter({ has: page.locator("svg") })
    .first()
    .hover();
  await page.getByRole("button", { name: "Expand diagram" }).click();
}
