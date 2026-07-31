import { type Page } from "@playwright/test";

const EXPAND = { role: "button", name: "Expand diagram" } as const;

/**
 * Open the diagram viewer for the `index`-th expandable diagram on the page.
 *
 * The hover is load-bearing: the button is `pointer-events: none` until
 * `figure.diagram:hover`, so a click without it never reaches the button. The
 * click is then scoped to the figure just hovered, since only that one accepts
 * pointer events and a page-wide match is ambiguous once a page has two
 * diagrams.
 *
 * Figures are located by the expand button they contain, which is simply the
 * set `initializeDiagramZoom` injects into — `figure.diagram:not(.diagram-error)`.
 * That includes hand-authored `<figure class="diagram">`, which is what the
 * `/diagram` fixture is; the filter narrows to expandable figures, not to
 * renderer-produced ones.
 */
export async function openDiagram(page: Page, index = 0) {
  const figure = page
    .locator("figure")
    .filter({ has: page.getByRole(EXPAND.role, { name: EXPAND.name }) })
    .nth(index);
  await figure.hover();
  await figure.getByRole(EXPAND.role, { name: EXPAND.name }).click();
}
