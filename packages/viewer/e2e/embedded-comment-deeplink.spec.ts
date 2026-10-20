import { test, expect, type Page } from "@playwright/test";
import {
  createComment,
  resolveAllComments,
  resolveDocumentId,
  setCommentStatus,
} from "./comment-helpers";

// Runs on its own page (documentId for "billing/invoices") so it never shares
// the comment DB rows with comments.spec.ts or comment-deeplink.spec.ts — the
// embedded and standalone servers share one DB.
const DOC_URL = "billing/invoices";
const DOC_PATH = "/billing/invoices";

const commentsLock = { lock: "e2e-comments:domain:default/billing#invoices" };

async function createPageComment(page: Page, body: string) {
  return createComment(page, { documentId: await resolveDocumentId(page, DOC_URL), body });
}

// Verbatim passage from the billing/invoices fixture body, so the inline
// comment anchors and renders an in-article highlight + sidebar thread.
const QUOTE = "management and processing";

async function createInlineComment(page: Page, body: string) {
  return createComment(page, {
    documentId: await resolveDocumentId(page, DOC_URL),
    body,
    quote: QUOTE,
  });
}

test.describe("Embedded comment deep links", commentsLock, () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(DOC_PATH);
    await resolveAllComments(page, await resolveDocumentId(page, DOC_URL));
  });

  test("inbound deep-link reveals a comment in embedded mode", async ({ page }) => {
    const body = `embedded-deeplink-${Date.now()}`;
    const id = await createPageComment(page, body);

    await page.goto("about:blank");
    await page.goto(`${DOC_PATH}#comment-${id}`);

    // Focus is set only by the deep-link reveal, so this proves the embedded
    // preview shell forwarded the hash to the viewer (inbound is mode-agnostic).
    const wrapper = page.locator(`#comment-${id}`);
    await expect(wrapper).toBeVisible();
    await expect(wrapper).toBeFocused();
  });

  test("copy-link button copies the comment permalink in embedded mode", async ({
    page,
    context,
  }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const body = `embedded-copy-${Date.now()}`;
    const id = await createPageComment(page, body);
    await page.goto(DOC_PATH);

    const thread = page.getByTestId("comment-thread").filter({ hasText: body });
    await expect(thread).toBeVisible();
    // In embedded mode the button copies the host page's URL with the comment hash.
    await thread.getByRole("button", { name: "Copy link" }).click();

    const copied = await page.evaluate(() => navigator.clipboard.readText());
    const expected = await page.evaluate(
      (id) => `${location.origin}${location.pathname}#comment-${id}`,
      id,
    );
    expect(copied).toBe(expected);
  });

  test("n/p mirrors the active comment into the URL hash in embedded mode", async ({ page }) => {
    const id1 = await createPageComment(page, `embedded-hash-a-${Date.now()}`);
    const id2 = await createPageComment(page, `embedded-hash-b-${Date.now()}`);
    await page.goto(DOC_PATH);
    await page.getByRole("region", { name: "Comments" }).waitFor();

    const lenBefore = await page.evaluate(() => history.length);

    await page.keyboard.press("n");
    await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id1}`);

    await page.keyboard.press("n");
    await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id2}`);

    // replaceState, not pushState — stepping must not grow history.
    expect(await page.evaluate(() => history.length)).toBe(lenBefore);
  });

  test("opening then closing an inline thread sets and clears the URL hash in embedded mode", async ({
    page,
  }) => {
    const id = await createInlineComment(page, `embedded-close-${Date.now()}`);
    await page.goto(DOC_PATH);

    const annotation = page.locator(`article rw-annotation[data-comment-id="${id}"]`).first();
    await expect(annotation).toBeVisible();
    await annotation.click();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id}`);

    // Closing the thread clears the mirrored hash (the outbound effect's clear
    // branch).
    await page.getByRole("button", { name: "Close comment" }).click();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
  });

  test("opening and closing a thread keeps the host's history state", async ({ page }) => {
    const id = await createInlineComment(page, `embedded-state-${Date.now()}`);
    await page.goto(DOC_PATH);
    // The shape React Router keeps there.
    const hostState = { usr: { from: "host" }, key: "k1", idx: 3 };
    await page.evaluate((state) => history.replaceState(state, ""), hostState);

    const annotation = page.locator(`article rw-annotation[data-comment-id="${id}"]`).first();
    await expect(annotation).toBeVisible();
    await annotation.click();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id}`);
    expect(await page.evaluate((): unknown => history.state)).toEqual(hostState);

    await page.getByRole("button", { name: "Close comment" }).click();
    await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
    expect(await page.evaluate((): unknown => history.state)).toEqual(hostState);
  });

  // A resolved comment, because only the viewer expands the resolved list: the
  // browser's own fragment navigation already scrolls to and focuses a visible
  // comment.
  async function createResolvedComment(page: Page, label: string) {
    const id = await createPageComment(page, `embedded-${label}-${Date.now()}`);
    await setCommentStatus(page, id, "resolved");
    return id;
  }

  const hideResolved = (page: Page) => page.getByRole("button", { name: "Hide resolved" });

  test("a hash-only host navigation reveals a comment in embedded mode", async ({ page }) => {
    const id = await createResolvedComment(page, "hash-only");
    await page.goto(DOC_PATH);
    await page.getByRole("region", { name: "Comments" }).waitFor();
    await expect(hideResolved(page)).toBeHidden();

    await page.evaluate((id) => {
      location.hash = `comment-${id}`;
    }, id);

    await expect(hideResolved(page)).toBeVisible();
  });

  test("Back to a comment hash reveals the comment again in embedded mode", async ({ page }) => {
    const id = await createResolvedComment(page, "back");
    await page.goto("about:blank");
    await page.goto(`${DOC_PATH}#comment-${id}`);
    await hideResolved(page).click();
    await page.evaluate(() => {
      location.hash = "elsewhere";
    });
    await expect(hideResolved(page)).toBeHidden();

    await page.goBack();

    await expect(hideResolved(page)).toBeVisible();
  });
});
