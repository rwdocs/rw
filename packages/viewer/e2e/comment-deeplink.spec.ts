import { test, expect, Page } from "@playwright/test";
import { resolveDocumentId } from "./comment-helpers";

// Runs on its own page (documentId for "getting-started") rather than the
// homepage, so it never shares the comment DB with comments.spec.ts — the two
// specs can run in parallel without racing on the same rows.
const DOC_URL = "getting-started";
const DOC_PATH = "/getting-started";
const QUOTE = "Configure your environment";

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

async function createInlineComment(page: Page, body: string, quote: string) {
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
    { body, quote, doc },
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto(DOC_PATH);
  await resolveAllComments(page);
});

test("deep-link to an open page comment scrolls to and focuses it", async ({ page }) => {
  const body = `deeplink-page-${Date.now()}`;
  const id = await createPageComment(page, body);

  await page.goto("about:blank");
  await page.goto(`${DOC_PATH}#comment-${id}`);

  const wrapper = page.locator(`#comment-${id}`);
  await expect(wrapper).toBeInViewport();
  await expect(wrapper).toBeFocused();
});

test("deep-link to a resolved comment expands the resolved disclosure", async ({ page }) => {
  const body = `deeplink-resolved-${Date.now()}`;
  const id = await createPageComment(page, body);
  await page.evaluate(async (id) => {
    await fetch(`/_api/comments/${id}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ status: "resolved" }),
    });
  }, id);

  await page.goto("about:blank");
  await page.goto(`${DOC_PATH}#comment-${id}`);

  await expect(page.getByRole("button", { name: "Hide resolved" })).toBeVisible();
  const wrapper = page.locator(`#comment-${id}`);
  await expect(wrapper).toBeInViewport();
});

test("deep-link to an unknown comment id is a silent no-op", async ({ page }) => {
  await page.goto("about:blank");
  await page.goto(`${DOC_PATH}#comment-00000000-0000-4000-8000-000000000000`);
  // Article still renders, no comment thread was targeted: nothing focused and
  // the page didn't jump anywhere meaningful.
  await expect(page.getByRole("region", { name: "Comments" })).toBeVisible();
  const focusedId = await page.evaluate(() => document.activeElement?.id ?? "");
  expect(focusedId).not.toMatch(/^comment-/);
  expect(await page.evaluate(() => window.scrollY)).toBeLessThan(50);
});

test("deep-link to an inline comment opens the sidebar and highlights it", async ({ page }) => {
  const body = `deeplink-inline-${Date.now()}`;
  const id = await createInlineComment(page, body, QUOTE);

  await page.goto("about:blank");
  await page.goto(`${DOC_PATH}#comment-${id}`);

  const sidebar = page.getByRole("complementary", { name: "Comments" });
  await expect(sidebar).toBeVisible();
  const active = page.locator('article rw-annotation[data-active="true"]');
  await expect(active.first()).toBeVisible();
  await expect(active.first()).toBeInViewport();

  // The passage lands ~⅓ down the viewport (scroll-margin-top: 33vh +
  // block:"start"), not centered and not jammed to the top. This leaves room
  // above for the pinned sidebar thread and matches where the eye rests when
  // arriving via a deeplink. The getting-started fixture is intentionally tall
  // enough (its "Tips" section) that the anchor has real scroll room below it,
  // so 33vh genuinely engages — without the scroll-margin, block:"start" would
  // land the highlight at top≈0 and fail the lower bound below.
  const { top, vh } = await active.first().evaluate((el) => ({
    top: el.getBoundingClientRect().top,
    vh: window.innerHeight,
  }));
  expect(top).toBeGreaterThan(vh * 0.2);
  expect(top).toBeLessThan(vh * 0.45);

  // The pinned sidebar thread header is still not clipped above the viewport.
  const cardTop = await sidebar
    .getByTestId("comment-thread")
    .first()
    .evaluate((el) => el.getBoundingClientRect().top);
  expect(cardTop).toBeGreaterThanOrEqual(0);
});

test("opening an inline thread reflects it in the URL without growing history", async ({
  page,
}) => {
  const body = `deeplink-replace-${Date.now()}`;
  const id = await createInlineComment(page, body, QUOTE);
  await page.goto(DOC_PATH);
  // Wait for the highlight to anchor after comments load.
  const annotation = page.locator(`article rw-annotation[data-comment-id="${id}"]`).first();
  await expect(annotation).toBeVisible();

  const lenBefore = await page.evaluate(() => history.length);
  await annotation.click();

  await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id}`);
  expect(await page.evaluate(() => history.length)).toBe(lenBefore);

  // Closing clears the comment hash.
  await page.getByRole("button", { name: "Close comment" }).click();
  await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
});

test("copy-link button copies the comment permalink", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const body = `deeplink-copy-${Date.now()}`;
  const id = await createPageComment(page, body);
  await page.goto(DOC_PATH);

  const wrapper = page.locator(`#comment-${id}`);
  await wrapper.getByRole("button", { name: "Copy link" }).click();

  const copied = await page.evaluate(() => navigator.clipboard.readText());
  const expected = await page.evaluate(
    (id) => `${location.origin}${location.pathname}#comment-${id}`,
    id,
  );
  expect(copied).toBe(expected);
});

test("a heading slugged like a comment hash still deep-links as a heading anchor", async ({
  page,
}) => {
  // `## Comment guidelines` on /advanced slugifies to `comment-guidelines`. It is
  // NOT a known comment, so it must route through the normal heading-scroll path
  // (the comment deep-link effect only claims hashes that match a loaded comment).
  await page.goto("about:blank");
  await page.goto("/advanced#comment-guidelines");

  // The heading sits below the fold; it only enters the viewport if it scrolled.
  await expect(page.locator("#comment-guidelines")).toBeInViewport();
  // And the comment sidebar is not triggered for a heading hash.
  await expect(page.getByRole("complementary", { name: "Comments" })).toBeHidden();
});

test("navigating to a different page clears the mirrored comment hash", async ({ page }) => {
  const id = await createInlineComment(page, `deeplink-nav-${Date.now()}`, QUOTE);
  await page.goto(DOC_PATH);
  const annotation = page.locator(`article rw-annotation[data-comment-id="${id}"]`).first();
  await expect(annotation).toBeVisible();
  await annotation.click();
  await expect.poll(() => page.evaluate(() => location.hash)).toBe(`#comment-${id}`);

  // SPA-navigate to another page — the mirrored comment hash must not linger.
  await page.locator("article").getByRole("link", { name: "Installation Guide" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Installation");
  await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
});

test("the page-comment tint follows keyboard navigation", async ({ page }) => {
  const id1 = await createPageComment(page, `nav-tint-a-${Date.now()}`);
  const id2 = await createPageComment(page, `nav-tint-b-${Date.now()}`);

  await page.goto("about:blank");
  await page.goto(`${DOC_PATH}#comment-${id1}`);

  // Deep-linked page comment carries the tint.
  await expect(page.locator(`#comment-${id1}`)).toHaveAttribute("data-linked", "true");

  // n moves the active comment — the tint follows, and only one card is tinted.
  await page.keyboard.press("n");
  await expect(page.locator(`#comment-${id2}`)).toHaveAttribute("data-linked", "true");
  await expect(page.locator(`#comment-${id1}`)).not.toHaveAttribute("data-linked", "true");
  await expect(page.locator('[data-testid="comment-thread"][data-linked="true"]')).toHaveCount(1);
});
