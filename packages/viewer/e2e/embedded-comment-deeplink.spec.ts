import { test, expect, Page } from "@playwright/test";
import { resolveDocumentId } from "./comment-helpers";

// Runs on its own page (documentId for "billing/invoices") so it never shares
// the comment DB rows with comments.spec.ts or comment-deeplink.spec.ts — the
// embedded and standalone servers share one DB.
const DOC_URL = "billing/invoices";
const DOC_PATH = "/billing/invoices";

test.describe.configure({ mode: "serial" });

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

async function createPageComment(page: Page, body: string) {
  const doc = await resolveDocumentId(page, DOC_URL);
  return page.evaluate(
    async ({ body, doc }) => {
      const res = await fetch("/_api/comments", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ documentId: doc, body }),
      });
      return (await res.json()).id as string;
    },
    { body, doc },
  );
}

// Verbatim passage from the billing/invoices fixture body, so the inline
// comment anchors and renders an in-article highlight + sidebar thread.
const QUOTE = "management and processing";

async function createInlineComment(page: Page, body: string) {
  const doc = await resolveDocumentId(page, DOC_URL);
  return page.evaluate(
    async ({ body, quote, doc }) => {
      const res = await fetch("/_api/comments", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ documentId: doc, body, quote }),
      });
      return (await res.json()).id as string;
    },
    { body, quote: QUOTE, doc },
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto(DOC_PATH);
  await resolveAllComments(page);
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
  // The button was previously hidden in embedded mode; assert it is both present
  // and functional (copies the host page's URL with the comment hash).
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
  // branch, now live in embedded mode).
  await page.getByRole("button", { name: "Close comment" }).click();
  await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
});

test("popstate to a comment hash re-focuses the comment in embedded mode", async ({ page }) => {
  const body = `embedded-popstate-${Date.now()}`;
  const id = await createPageComment(page, body);

  // Land on the comment (fresh-load inbound), then push a different hash so
  // there is a history entry to come Back from. Bounce through about:blank so the
  // deep-link goto is a clean cross-document load (matching the inbound test).
  await page.goto("about:blank");
  await page.goto(`${DOC_PATH}#comment-${id}`);
  await expect(page.locator(`#comment-${id}`)).toBeFocused();

  // Move to a non-comment hash and blur the comment. Blur is the key precondition:
  // a hash navigation alone does not move focus, so without it a stale focus would
  // make the post-Back assertion pass even if the comment is never re-revealed.
  await page.evaluate(() => {
    history.pushState(null, "", "#elsewhere");
    (document.activeElement as HTMLElement | null)?.blur();
  });
  await expect.poll(() => page.evaluate(() => location.hash)).toBe("#elsewhere");
  await expect(page.locator(`#comment-${id}`)).not.toBeFocused();

  // Back → window.location.hash returns to #comment-<id> → popstate re-reveals
  // (re-focuses) the comment, even though it is still the linked target.
  await page.goBack();
  await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id}`);
  await expect(page.locator(`#comment-${id}`)).toBeFocused();
});
