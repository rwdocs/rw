import { test, expect } from "@playwright/test";

test.describe("Diagram zoom popup", () => {
  test.use({ viewport: { width: 1200, height: 800 } });

  test("expand button opens a dialog with the diagram", async ({ page }) => {
    await page.goto("/diagram");

    // The expand button is hover-revealed (opacity), so force the click.
    const expand = page.getByRole("button", { name: "Expand diagram" });
    await expand.click({ force: true });

    const dialog = page.getByRole("dialog", { name: "Diagram viewer" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByTestId("diagram-zoom-content").locator("svg")).toBeVisible();
  });

  test("zoom in magnifies the diagram by shrinking its viewBox", async ({ page }) => {
    await page.goto("/diagram");
    await page.getByRole("button", { name: "Expand diagram" }).click({ force: true });

    // Zoom is driven by the SVG's viewBox (crisp vector re-render), not by
    // resizing or CSS-scaling a box. Zooming in shows a smaller slice of the
    // diagram, so the viewBox width must shrink.
    const svg = page.getByTestId("diagram-zoom-content").locator("svg");
    const vbWidth = async () =>
      Number((await svg.getAttribute("viewBox"))!.trim().split(/[\s,]+/)[2]);
    const before = await vbWidth();
    await page.getByRole("button", { name: "Zoom in" }).click();
    const after = await vbWidth();
    expect(after).toBeLessThan(before);
  });

  test("Escape closes the popup", async ({ page }) => {
    await page.goto("/diagram");
    await page.getByRole("button", { name: "Expand diagram" }).click({ force: true });

    const dialog = page.getByRole("dialog", { name: "Diagram viewer" });
    await expect(dialog).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });

  test("Close button closes the popup", async ({ page }) => {
    await page.goto("/diagram");
    await page.getByRole("button", { name: "Expand diagram" }).click({ force: true });

    await page.getByRole("button", { name: "Close" }).click();
    await expect(page.getByRole("dialog", { name: "Diagram viewer" })).toBeHidden();
  });

  test("clone keeps id-scoped <style> rules styling it (mermaid-shaped svg)", async ({ page }) => {
    await page.goto("/diagram");
    await page.getByRole("button", { name: "Expand diagram" }).click({ force: true });

    const rect = page.getByTestId("diagram-zoom-content").locator("svg rect");
    await expect(rect).toBeVisible();
    // The fixture styles its rect via a rule scoped to the SVG root id.
    // Namespacing the clone's ids must rename that selector in lockstep, or the
    // rect falls back to SVG's default black fill.
    const fill = await rect.evaluate((el) => getComputedStyle(el).fill);
    expect(fill).toBe("rgb(238, 238, 255)"); // #eef
  });
});
