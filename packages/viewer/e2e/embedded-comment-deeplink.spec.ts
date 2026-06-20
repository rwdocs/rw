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

test("copy-link button is hidden in embedded mode", async ({ page }) => {
  const body = `embedded-nocopy-${Date.now()}`;
  await createPageComment(page, body);
  await page.goto(DOC_PATH);

  const thread = page.getByTestId("comment-thread").filter({ hasText: body });
  await expect(thread).toBeVisible();
  await expect(thread.getByRole("button", { name: "Copy link" })).toHaveCount(0);
});
