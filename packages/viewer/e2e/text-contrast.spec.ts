import { test, expect, type Locator, type Page } from "@playwright/test";
import type { Comment } from "../src/types/comments";

// Use authored colors, not antialiased glyph pixels; composite translucent
// surfaces in the browser so dark alert panels and reply backgrounds count.
async function contrast(target: Locator, pseudo: string | null = null): Promise<number> {
  return target.evaluate((el, pseudoElement) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "white";
    ctx.fillRect(0, 0, 1, 1);
    const ancestors: Element[] = [];
    for (let node: Element | null = el; node; node = node.parentElement) ancestors.unshift(node);
    for (const node of ancestors) {
      const style = getComputedStyle(node);
      if (style.backgroundImage !== "none" || style.opacity !== "1") {
        throw new Error("Contrast helper requires flat backgrounds and full element opacity");
      }
      ctx.fillStyle = style.backgroundColor;
      ctx.fillRect(0, 0, 1, 1);
    }
    const background = ctx.getImageData(0, 0, 1, 1).data;
    const textStyle = getComputedStyle(el, pseudoElement);
    if (textStyle.opacity !== "1") throw new Error("Text must not have reduced opacity");
    ctx.fillStyle = textStyle.color;
    ctx.fillRect(0, 0, 1, 1);
    const foreground = ctx.getImageData(0, 0, 1, 1).data;
    function luminance(rgb: Uint8ClampedArray) {
      return [0.2126, 0.7152, 0.0722].reduce((sum, weight, i) => {
        const channel = rgb[i] / 255;
        const linear = channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
        return sum + weight * linear;
      }, 0);
    }
    const values = [luminance(background), luminance(foreground)].sort((a, b) => b - a);
    return (values[0] + 0.05) / (values[1] + 0.05);
  }, pseudo);
}

async function open(page: Page) {
  await page.goto("./text-contrast");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Text contrast");
  await page.addStyleTag({ content: "* { transition: none !important; }" });
}

async function textContrast(target: Locator, pseudo: string | null = null) {
  await expect(target).toBeVisible();
  return contrast(target, pseudo);
}

for (const colorScheme of ["light", "dark"] as const) {
  test.describe(`Text contrast (${colorScheme})`, () => {
    test.use({ viewport: { width: 1600, height: 1000 }, colorScheme });

    test("all alert titles meet normal-text contrast", async ({ page }) => {
      await open(page);
      const article = page.getByRole("article");
      for (const title of ["Note", "Tip", "Important", "Warning", "Caution"]) {
        expect(
          await textContrast(article.getByText(title, { exact: true })),
        ).toBeGreaterThanOrEqual(4.5);
      }
    });

    test("enabled solid button labels meet contrast at rest and on hover", async ({ page }) => {
      await page.route("**/_api/comments", async (route) => {
        if (route.request().method() === "POST") {
          await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
        } else {
          await route.fallback();
        }
      });
      await open(page);
      await page.getByPlaceholder("Write a comment...").fill("Contrast check");
      const primary = page.getByRole("button", { name: "Comment", exact: true });
      expect(await textContrast(primary)).toBeGreaterThanOrEqual(4.5);
      await primary.hover();
      expect(await textContrast(primary)).toBeGreaterThanOrEqual(4.5);
      await primary.click();
      const failure = page.getByRole("alert");
      expect(await textContrast(failure)).toBeGreaterThanOrEqual(4.5);
      await failure.getByRole("button", { name: "Dismiss", exact: true }).click();
      const danger = page.getByRole("button", { name: "Retry", exact: true });
      expect(await textContrast(danger)).toBeGreaterThanOrEqual(4.5);
      await danger.hover();
      expect(await textContrast(danger)).toBeGreaterThanOrEqual(4.5);
    });

    test("comment timestamps and placeholders meet normal-text contrast", async ({ page }) => {
      await page.route("**/_api/comments?**", async (route) => {
        const now = Date.now();
        const documentId = new URL(route.request().url()).searchParams.get("documentId")!;
        const comment: Comment = {
          id: "contrast-thread",
          documentId,
          author: { id: "reader", name: "Reader" },
          body: "Comment with readable metadata",
          selectors: [],
          status: "open",
          createdAt: new Date(now - 90 * 60_000).toISOString(),
          updatedAt: new Date(now).toISOString(),
          canDelete: false,
          canRestore: false,
          canResolve: true,
        };
        await route.fulfill({
          json: [
            comment,
            {
              ...comment,
              id: "contrast-reply",
              parentId: comment.id,
              body: "Reply with readable metadata",
              createdAt: new Date(now - 150 * 60_000).toISOString(),
              canResolve: false,
            },
          ],
        });
      });
      await open(page);
      const comments = page.getByRole("region", { name: "Comments" });
      expect(
        await textContrast(comments.getByText("1h ago", { exact: true })),
      ).toBeGreaterThanOrEqual(4.5);
      expect(
        await textContrast(comments.getByText("2h ago", { exact: true })),
      ).toBeGreaterThanOrEqual(4.5);
      expect(
        await textContrast(page.getByPlaceholder("Write a comment..."), "::placeholder"),
      ).toBeGreaterThanOrEqual(4.5);
    });
  });
}
