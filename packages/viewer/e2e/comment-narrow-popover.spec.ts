import { test, expect, Page } from "@playwright/test";
import { resolveDocumentId } from "./comment-helpers";

// Narrow viewport: below the 952px comments breakpoint, so the margin aside is
// hidden and the CommentPopover is the only inline-thread surface.
test.use({ viewport: { width: 700, height: 900 } });
test.describe.configure({ mode: "serial" });

// Own page so rows never race comments.spec (homepage) or the deeplink/nav specs.
const DOC_URL = "advanced";
const DOC_PATH = "/advanced";

// Select `text` inside the article and fire the mouseup the viewer listens for.
// Copied from comments.spec.ts.
async function selectText(page: Page, text: string) {
  await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const fullText = article.textContent ?? "";
    const startInDoc = fullText.indexOf(targetText);
    if (startInDoc === -1) throw new Error(`text "${targetText}" not found`);
    const endInDoc = startInDoc + targetText.length;
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    let offset = 0;
    let startNode: Text | null = null;
    let startOffset = 0;
    let endNode: Text | null = null;
    let endOffset = 0;
    while (walker.nextNode()) {
      const node = walker.currentNode as Text;
      const len = node.data.length;
      if (!startNode && offset + len > startInDoc) {
        startNode = node;
        startOffset = startInDoc - offset;
      }
      if (startNode && offset + len >= endInDoc) {
        endNode = node;
        endOffset = endInDoc - offset;
        break;
      }
      offset += len;
    }
    if (!startNode || !endNode) throw new Error(`couldn't build range for "${targetText}"`);
    const range = document.createRange();
    range.setStart(startNode, startOffset);
    range.setEnd(endNode, endOffset);
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
  }, text);
}

// Seed an inline comment anchored to `quote`. Verified payload shape from
// comment-deeplink.spec.ts: { documentId, body, quote }.
async function seedInline(page: Page, body: string, quote: string): Promise<string> {
  const doc = await resolveDocumentId(page, DOC_URL);
  return page.evaluate(
    async ({ body, quote, doc }) => {
      const res = await fetch("/_api/comments", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ documentId: doc, body, quote }),
      });
      if (!res.ok) throw new Error(`POST /_api/comments -> ${res.status}`);
      return (await res.json()).id as string;
    },
    { body, quote, doc },
  );
}

// Resolve every open comment so each test starts clean. Copied from
// comment-deeplink.spec.ts.
async function resolveAllComments(page: Page) {
  const doc = await resolveDocumentId(page, DOC_URL);
  const open = await page.evaluate(async (docId) => {
    const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}&status=open`);
    return res.json();
  }, doc);
  for (const c of open) {
    await page.evaluate(async (id) => {
      await fetch(`/_api/comments/${id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ status: "resolved" }),
      });
    }, c.id);
  }
}

test.beforeEach(async ({ page }) => {
  await page.goto(DOC_PATH);
  await resolveAllComments(page);
});

// The popover is a labelled group (role="group" aria-label="Comments"), distinct
// from the wide-screen <aside> complementary "Comments" landmark and from the
// page-comments <section> region (also named "Comments").
const popover = (page: Page) => page.getByRole("group", { name: "Comments" });

test("tapping a highlight opens the thread in a popover; no margin aside", async ({ page }) => {
  await seedInline(page, "Needs a definition here", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();

  await expect(popover(page)).toBeVisible();
  await expect(popover(page).getByText("Needs a definition here")).toBeVisible();
  // The 320px margin aside is not rendered at this width.
  await expect(page.getByRole("complementary", { name: "Comments" })).toHaveCount(0);
});

test("Escape dismisses the popover", async ({ page }) => {
  await seedInline(page, "Dismiss me", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();
  await expect(popover(page)).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(popover(page)).toBeHidden();
});

test("Escape inside the reply box blurs the field without closing the thread", async ({ page }) => {
  await seedInline(page, "Has a reply box", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();
  const reply = popover(page).getByPlaceholder("Write a reply...");
  await reply.fill("draft reply text");
  await reply.press("Escape");

  // The thread stays open and the in-progress reply draft is preserved — Escape
  // only blurs the field; it does not tear the whole popover down.
  await expect(popover(page)).toBeVisible();
  await expect(reply).toHaveValue("draft reply text");
});

test("Escape during an IME composition does not close the popover", async ({ page }) => {
  await seedInline(page, "Has a reply box", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();
  const reply = popover(page).getByPlaceholder("Write a reply...");
  await reply.focus();

  // Simulate Escape cancelling an in-progress IME composition (isComposing=true).
  // CommentForm deliberately skips its blur/preventDefault while composing, so the
  // popover's own handler must also ignore it rather than tear the thread down.
  await reply.evaluate((el) =>
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", isComposing: true, bubbles: true }),
    ),
  );

  await expect(popover(page)).toBeVisible();
});

test("clicking outside (not on a highlight) dismisses the popover", async ({ page }) => {
  await seedInline(page, "Outside click", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();
  await expect(popover(page)).toBeVisible();

  // Click the page heading — outside the popover and not a highlight.
  await page.getByRole("heading", { name: "Advanced Topics" }).click();
  await expect(popover(page)).toBeHidden();
});

test("selecting text and adding a comment shows the draft in the popover", async ({ page }) => {
  await selectText(page, "Performance Tips");
  await page.getByRole("button", { name: "Add comment" }).click();

  await expect(popover(page)).toBeVisible();
  const draft = popover(page).getByRole("textbox");
  await draft.fill("A brand new inline comment");
  await popover(page).getByRole("button", { name: "Comment", exact: true }).click();

  await expect(popover(page).getByText("A brand new inline comment")).toBeVisible();
});

test("tapping a second highlight switches the popover instead of closing it", async ({ page }) => {
  // Two passages far apart vertically (top Topics list vs the bottom Performance
  // Tips list) so the first comment's popover does not overlap — and intercept
  // the click on — the second highlight.
  await seedInline(page, "First comment", "Performance optimization");
  await seedInline(page, "Second comment", "Enable compression");
  await page.reload();

  const highlights = page.locator("rw-annotation");
  await highlights.nth(0).click();
  await expect(popover(page).getByText("First comment")).toBeVisible();

  await highlights.nth(1).click();
  // Switched, not closed.
  await expect(popover(page).getByText("Second comment")).toBeVisible();
  await expect(popover(page).getByText("First comment")).toHaveCount(0);
});

test("n opens the popover on the navigated comment", async ({ page }) => {
  await seedInline(page, "Nav target comment", "Performance optimization");
  await page.reload();

  // Wait for the highlight to anchor before navigating, else `n` fires against an
  // empty navigable list (the comment hasn't loaded yet).
  await expect(page.locator("rw-annotation").first()).toBeVisible();
  await page.keyboard.press("n");

  await expect(popover(page)).toBeVisible();
  await expect(popover(page).getByText("Nav target comment")).toBeVisible();
});

test("r moves focus into the popover's reply box", async ({ page }) => {
  await seedInline(page, "Focus my reply", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();
  await expect(popover(page)).toBeVisible();

  // `r` is a global shortcut that focuses the active thread's reply box; it must
  // reach the box rendered inside the popover (not only the wide aside).
  await page.keyboard.press("r");
  await expect(popover(page).getByPlaceholder("Write a reply...")).toBeFocused();
});

test("a reply draft survives switching threads in the popover", async ({ page }) => {
  await seedInline(page, "First thread", "Performance optimization");
  await seedInline(page, "Second thread", "Enable compression");
  await page.reload();

  const highlights = page.locator("rw-annotation");
  await highlights.nth(0).click();
  await popover(page).getByPlaceholder("Write a reply...").fill("draft for first");

  // Switch to the second thread (the draft is per-thread, so its box is empty)…
  await highlights.nth(1).click();
  await expect(popover(page).getByText("Second thread")).toBeVisible();
  await expect(popover(page).getByPlaceholder("Write a reply...")).toHaveValue("");

  // …then back to the first: the draft is restored, not lost on the remount.
  await highlights.nth(0).click();
  await expect(popover(page).getByText("First thread")).toBeVisible();
  await expect(popover(page).getByPlaceholder("Write a reply...")).toHaveValue("draft for first");
});

test("at wide widths the margin aside is used and the popover is absent", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto(DOC_PATH);
  await resolveAllComments(page);
  await seedInline(page, "Wide comment", "Performance optimization");
  await page.reload();

  await page.locator("rw-annotation").first().click();

  const aside = page.getByRole("complementary", { name: "Comments" });
  await expect(aside).toBeVisible();
  await expect(aside.getByText("Wide comment")).toBeVisible();
  await expect(popover(page)).toHaveCount(0);
});
