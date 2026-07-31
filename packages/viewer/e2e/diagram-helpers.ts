import { type Page } from "@playwright/test";

const EXPAND = { role: "button", name: "Expand diagram" } as const;

/**
 * Open the diagram viewer for the `index`-th diagram on the page.
 *
 * The figure is identified by the expand button it contains rather than by a
 * class or by holding an `<svg>`: the button is injected only into rendered
 * diagram figures, whereas a hand-authored `<figure>` with an inline SVG is a
 * supported shape that never gets one. Hovering is required, not cosmetic —
 * the button is `pointer-events: none` until `figure.diagram:hover` — and the
 * click is scoped to the figure just hovered, since only that one accepts
 * pointer events and a page-wide match would be ambiguous with two diagrams.
 */
export async function openDiagram(page: Page, index = 0) {
  const figure = page
    .locator("figure")
    .filter({ has: page.getByRole(EXPAND.role, { name: EXPAND.name }) })
    .nth(index);
  await figure.hover();
  await figure.getByRole(EXPAND.role, { name: EXPAND.name }).click();
}
