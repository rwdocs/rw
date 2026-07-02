import { test, expect, Page } from "@playwright/test";
import { resolveDocumentId } from "./comment-helpers";

// Wide viewport so the right comment sidebar (and its nav badge) is visible.
test.use({ viewport: { width: 1400, height: 800 } });
test.describe.configure({ mode: "serial" });

// Dedicated page so we never share comment rows with other specs (installation
// is unused by the other comment specs, which claim "/", "/advanced",
// "/api/endpoints" and "/getting-started/configuration").
const PAGE_PATH = "/getting-started/installation";
const PAGE_URL = "getting-started/installation";

// Six distinct prose passages, in document order.
const ANCHORS = [
  "install the platform on your system",
  "Before installing",
  "verify it works",
  "version number printed",
  "encounter issues",
  "operating systems and architectures",
];

async function postComment(page: Page, payload: Record<string, unknown>): Promise<string> {
  return page.evaluate(async (body) => {
    const res = await fetch("/_api/comments", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`create comment failed: ${res.status}`);
    return (await res.json()).id as string;
  }, payload);
}

async function resolveAllComments(page: Page, documentId: string) {
  await page.evaluate(async (docId) => {
    // GET /_api/comments returns a bare JSON array (Json<Vec<CommentResponse>>).
    const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}`);
    const comments: Array<{ id: string; status: string }> = await res.json();
    for (const c of comments) {
      if (c.status !== "resolved") {
        await fetch(`/_api/comments/${c.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ status: "resolved" }),
        });
      }
    }
  }, documentId);
}

function sidebar(page: Page) {
  return page.getByRole("complementary", { name: "Comments" });
}

test.beforeEach(async ({ page }) => {
  await page.goto(PAGE_PATH);
  await page.getByRole("article").waitFor();
  const docId = await resolveDocumentId(page, PAGE_URL);
  await resolveAllComments(page, docId);
  // Seed six inline comments on distinct passages via the API.
  for (const [i, exact] of ANCHORS.entries()) {
    await postComment(page, {
      documentId: docId,
      body: `seed ${i}`,
      selectors: [{ type: "TextQuoteSelector", exact, prefix: "", suffix: "" }],
    });
  }
  await page.reload();
  await page.getByRole("article").waitFor();
  // Wait until all six seeds have loaded and anchored before any test drives the
  // keyboard: `n` from idle is a single non-retried keypress, so it must not fire
  // against an empty comment store.
  await expect(page.locator("article rw-annotation")).toHaveCount(6);
});

test("resolving the active comment keeps its badge slot, then drops out on next", async ({
  page,
}) => {
  // Activate the first comment in document order (idle → first).
  await page.keyboard.press("n");
  const bar = sidebar(page);
  await expect(bar.getByText("1 / 6", { exact: true })).toBeVisible();

  // Resolve it — it stays active and keeps its slot (bug: jumped to "6 / 6").
  await bar.getByRole("button", { name: "Resolve", exact: true }).click();
  await expect(bar.getByRole("button", { name: "Reopen", exact: true })).toBeVisible();
  await expect(bar.getByText("1 / 6", { exact: true })).toBeVisible();
  // The resolved comment is kept in the wrapped set because it is active (still 6
  // wrappers), and its passage is the highlighted/active one.
  await expect(page.locator("article rw-annotation")).toHaveCount(6);
  await expect(page.locator('article rw-annotation[data-active="true"]')).toHaveCount(1);

  // Navigate to the next comment: the resolved one leaves the set.
  await bar.getByRole("button", { name: "Next comment" }).click();
  await expect(bar.getByText("1 / 5", { exact: true })).toBeVisible();
});

test("after resolving, a re-render re-wraps only the remaining open highlights", async ({
  page,
}) => {
  // Resolve the first comment and step off it so it is no longer active.
  await page.keyboard.press("n");
  const bar = sidebar(page);
  await bar.getByRole("button", { name: "Resolve", exact: true }).click();
  await expect(bar.getByRole("button", { name: "Reopen", exact: true })).toBeVisible();
  await bar.getByRole("button", { name: "Next comment" }).click();
  await expect(bar.getByText("1 / 5", { exact: true })).toBeVisible();

  // A reload re-renders the article (wipes all wrappers). The wrap effect's
  // DOM-truth early-out must still re-wrap — exactly the five open comments, not
  // the resolved one (which is no longer active).
  await page.reload();
  await page.getByRole("article").waitFor();
  await expect(page.locator("article rw-annotation")).toHaveCount(5);
});
