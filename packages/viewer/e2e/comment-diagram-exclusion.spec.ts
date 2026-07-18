import { test, expect, type Page } from "@playwright/test";
import { selectText } from "./comment-helpers";

test.describe("diagram comment exclusion", () => {
  test("selecting a diagram label shows no Add comment popover", async ({ page }) => {
    await page.goto("/diagram");
    await page.getByRole("article").waitFor();

    // Positive control: a prose selection DOES offer the popover.
    await selectText(page, "raw diagram figure");
    await expect(page.getByRole("button", { name: "Add comment" })).toBeVisible();

    // Clear, then select the SVG label — the popover must not appear.
    await page.mouse.click(1, 1);
    await selectText(page, "Big diagram");
    await expect(page.getByRole("button", { name: "Add comment" })).toBeHidden();
  });

  test("selecting a label inside a wrapped diagram shows no Add comment popover", async ({
    page,
  }) => {
    // The shape real users get: the SVG lives in an <rw-diagram> shadow root, so
    // its labels are outside `article.textContent` and `selectText` can't reach
    // them. Selecting one directly must still be refused.
    await page.goto("/diagram-collision");
    await page.getByRole("article").waitFor();

    // Positive control: a prose selection DOES offer the popover.
    await selectText(page, "internal ids collide");
    await expect(page.getByRole("button", { name: "Add comment" })).toBeVisible();

    await page.mouse.click(1, 1);
    await selectDiagramLabel(page, "Collision label");
    await expect(page.getByRole("button", { name: "Add comment" })).toBeHidden();
  });
});

/**
 * Select an SVG `<text>` label living inside an `<rw-diagram>` shadow root and
 * release with a synthetic mouseup. The resulting Range's
 * `commonAncestorContainer` is a shadow node, which no light-DOM `article`
 * contains — the case the first popover guard exists for.
 */
async function selectDiagramLabel(page: Page, label: string) {
  await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const node = [...article.querySelectorAll("rw-diagram")]
      .flatMap((host) => [...(host.shadowRoot?.querySelectorAll("text") ?? [])])
      .find((t) => t.textContent === targetText)?.firstChild as Text | undefined;
    if (!node) throw new Error(`no shadow-root label "${targetText}"`);

    const range = document.createRange();
    range.setStart(node, 0);
    range.setEnd(node, node.data.length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);

    const rect = range.getBoundingClientRect();
    article.dispatchEvent(
      new MouseEvent("mouseup", {
        bubbles: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
  }, label);
}
