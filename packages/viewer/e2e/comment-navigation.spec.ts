import { test, expect, Page } from "@playwright/test";

// Wide viewport so the right comment sidebar is visible.
test.use({ viewport: { width: 1400, height: 800 } });
test.describe.configure({ mode: "serial" });

// These tests run on a dedicated page (its own documentId) so they never share
// comment rows with comments.spec.ts — the two spec files run in parallel and
// both create/resolve comments, so using the same documentId would let one
// file's resolveAll close the other's in-flight comments. The intro line of
// this page is the passage we anchor inline comments to.
const PAGE_PATH = "/getting-started/configuration";
const PAGE_DOC_ID = "getting-started/configuration";
const ANCHOR_TEXT = "configure the platform";

/** Select a text range inside the article and trigger the selection popover. */
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

async function createInlineComment(page: Page, targetText: string, body: string) {
  await selectText(page, targetText);
  await page.getByRole("button", { name: "Add comment" }).click();
  const sidebar = page.getByRole("complementary", { name: "Comments" });
  await sidebar.getByPlaceholder("Write a comment...").fill(body);
  await sidebar.getByRole("button", { name: "Comment", exact: true }).click();
  await expect(sidebar.getByPlaceholder("Write a comment...")).not.toBeVisible();
}

async function createPageComment(page: Page, body: string) {
  const section = page.getByRole("region", { name: "Comments" });
  await section.getByPlaceholder("Write a comment...").fill(body);
  await section.getByRole("button", { name: "Comment", exact: true }).click();
}

/** POST a comment to the REST API from the browser context. Returns its id. */
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

/** Create a page-level comment (no selectors → no anchor) with `replyCount`
 *  replies. Returns the root comment id. */
async function createPageCommentWithReplies(
  page: Page,
  documentId: string,
  body: string,
  replyCount: number,
): Promise<string> {
  const rootId = await postComment(page, { documentId, body });
  for (let i = 0; i < replyCount; i++) {
    await postComment(page, { documentId, parentId: rootId, body: `reply number ${i + 1}` });
  }
  return rootId;
}

/** Create a top-level comment whose stored selectors cannot anchor to the
 *  current document — the viewer treats it as an orphaned inline comment and
 *  surfaces it in the page-comments timeline. Returns the new comment id. */
async function createOrphanComment(page: Page, documentId: string, body: string): Promise<string> {
  return postComment(page, {
    documentId,
    body,
    selectors: [
      {
        type: "TextQuoteSelector",
        exact: "a passage that is definitely not present anywhere on this page",
        prefix: "",
        suffix: "",
      },
    ],
  });
}

async function resolveAllComments(page: Page, documentId: string) {
  await page.evaluate(async (docId) => {
    const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}`);
    const comments = await res.json();
    for (const c of comments) {
      if (c.status === "open") {
        await fetch(`/_api/comments/${c.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ status: "resolved" }),
        });
      }
    }
  }, documentId);
}

async function waitForHighlights(page: Page) {
  await expect(async () => {
    const count = await page.evaluate(
      () => document.querySelectorAll("article rw-annotation").length,
    );
    expect(count).toBeGreaterThan(0);
  }).toPass({ timeout: 10000 });
}

/** data-comment-id of the inline highlight currently marked active, if any. */
function activeHighlightId(page: Page) {
  return page.evaluate(
    () =>
      document
        .querySelector("article rw-annotation[data-active='true']")
        ?.getAttribute("data-comment-id") ?? null,
  );
}

/** The visually-hidden aria-live region. It is sr-only (clipped), so it is
 *  outside Playwright's default accessibility tree — query it with includeHidden. */
function liveRegion(page: Page) {
  return page.getByRole("status", { includeHidden: true });
}

/** Reload the page so the comment store starts fresh (activeId null = idle),
 *  with all DB-persisted comments loaded and inline highlights anchored. This
 *  mirrors how a reviewer actually arrives: open the page, then press a key. */
async function reloadIdle(page: Page) {
  // Reach a true idle state. Creating/opening a comment leaves a #comment-<id>
  // hash in the URL (the deep-link feature mirrors the active thread), and a bare
  // reload would re-activate that thread on load. Bounce through about:blank so
  // the page reloads at the bare path with no hash and no active comment.
  await page.goto("about:blank");
  await page.goto(PAGE_PATH);
  await page.getByRole("article").waitFor();
  await page.getByRole("region", { name: "Comments" }).waitFor();
  await waitForHighlights(page);
}

test.describe("Comment keyboard navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(PAGE_PATH);
    await resolveAllComments(page, PAGE_DOC_ID);
    await page.reload();
    await page.getByRole("article").waitFor();
    // The page-comments <section> only mounts once the config request has
    // flipped comments on. Selecting text before then misses the one-shot
    // mouseup, so the "Add comment" popover never appears. Wait for it.
    await page.getByRole("region", { name: "Comments" }).waitFor();
  });

  test("n from idle opens the first comment and scrolls it into view", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "first inline");
    await reloadIdle(page);

    await page.keyboard.press("n");

    await expect(page.getByRole("complementary", { name: "Comments" })).toBeVisible();
    expect(await activeHighlightId(page)).not.toBeNull();
    await expect(liveRegion(page)).toContainText("Comment 1 of 1");
    // The active highlight is scrolled into view (centered) on the jump.
    await expect(page.locator("article rw-annotation[data-active='true']")).toBeInViewport();
  });

  test("n steps through inline then page comments and wraps", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    await createPageComment(page, "page level one");
    await expect(page.getByRole("region", { name: "Comments" })).toContainText("page level one");
    await reloadIdle(page);

    await page.keyboard.press("n"); // idle → first (inline, 1 of 2)
    await expect(liveRegion(page)).toContainText("Comment 1 of 2");
    const firstActive = await activeHighlightId(page);
    expect(firstActive).not.toBeNull();

    await page.keyboard.press("n"); // → page comment (2 of 2)
    await expect(liveRegion(page)).toContainText("Comment 2 of 2");
    // The page comment is not an inline highlight, so no active highlight now.
    expect(await activeHighlightId(page)).toBeNull();

    await page.keyboard.press("n"); // wraps → inline (1 of 2)
    await expect(liveRegion(page)).toContainText("Comment 1 of 2");
    expect(await activeHighlightId(page)).toBe(firstActive);
  });

  test("n steps onto orphaned-inline comments, highlighting each and advancing", async ({
    page,
  }) => {
    // One anchored inline thread + two orphaned-inline threads (stored selectors
    // that no longer match any text — what a content edit looks like to the
    // viewer). Orphans render in the page-comments timeline and are valid n/p
    // targets; they must highlight and let navigation continue.
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    // Created in order, so orphan A sorts before orphan B in the timeline (open
    // page/orphan threads order by createdAt) and thus in n/p order after the
    // inline thread.
    await createOrphanComment(page, PAGE_DOC_ID, "orphan A body");
    await createOrphanComment(page, PAGE_DOC_ID, "orphan B body");
    await reloadIdle(page);

    // Locate each timeline card by its unique body text (the cards carry the
    // semantic data-testid; data-linked is the tint state with no role/text
    // equivalent, so it's asserted as an attribute on the located card).
    const section = page.getByRole("region", { name: "Comments" });
    const orphanACard = section.getByTestId("comment-thread").filter({ hasText: "orphan A body" });
    const orphanBCard = section.getByTestId("comment-thread").filter({ hasText: "orphan B body" });
    await expect(orphanACard).toBeVisible();
    await expect(orphanBCard).toBeVisible();
    // Baseline: nothing is tinted before navigation, so the data-linked checks
    // below are differential (they'd fail if the tint were applied uncondi-
    // tionally rather than following the active comment).
    await expect(orphanACard).not.toHaveAttribute("data-linked", "true");
    await expect(orphanBCard).not.toHaveAttribute("data-linked", "true");

    // idle → inline (1 of 3)
    await page.keyboard.press("n");
    await expect(liveRegion(page)).toContainText("Comment 1 of 3");
    const firstActive = await activeHighlightId(page);
    expect(firstActive).not.toBeNull();

    // → first orphan (2 of 3): no inline highlight, but the timeline card is tinted.
    await page.keyboard.press("n");
    await expect(liveRegion(page)).toContainText("Comment 2 of 3");
    expect(await activeHighlightId(page)).toBeNull();
    await expect(orphanACard).toHaveAttribute("data-linked", "true");

    // → second orphan (3 of 3): stepping onto an orphan keeps it active, so
    // navigation advances to the next thread instead of re-entering from idle,
    // and the tint moves from orphan A to orphan B.
    await page.keyboard.press("n");
    await expect(liveRegion(page)).toContainText("Comment 3 of 3");
    await expect(orphanBCard).toHaveAttribute("data-linked", "true");
    await expect(orphanACard).not.toHaveAttribute("data-linked", "true");

    // wraps → inline (1 of 3)
    await page.keyboard.press("n");
    await expect(liveRegion(page)).toContainText("Comment 1 of 3");
    expect(await activeHighlightId(page)).toBe(firstActive);
  });

  test("p from idle jumps to the last comment", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    await createPageComment(page, "page level last");
    await expect(page.getByRole("region", { name: "Comments" })).toContainText("page level last");
    await reloadIdle(page);

    await page.keyboard.press("p"); // idle → last (the page comment)

    await expect(liveRegion(page)).toContainText("Comment 2 of 2");
    expect(await activeHighlightId(page)).toBeNull();
  });

  test("typing n in the comment form does not navigate", async ({ page }) => {
    await createInlineComment(page, ANCHOR_TEXT, "inline one");
    await reloadIdle(page);

    const textarea = page
      .getByRole("region", { name: "Comments" })
      .getByPlaceholder("Write a comment...");
    await textarea.click();
    await textarea.pressSequentially("nano notes");

    await expect(textarea).toHaveValue("nano notes");
    expect(await activeHighlightId(page)).toBeNull();
    await expect(liveRegion(page)).toHaveText("");
  });

  test("n top-aligns a tall page comment so its root stays visible", async ({ page }) => {
    // A page comment with many replies renders a thread card taller than the
    // 800px viewport. Centering such a card pushes the root comment off-screen
    // above, so navigation must top-align (block: "start") to keep it visible.
    //
    // An inline comment is created first so an in-article highlight exists (the
    // idle-reload helper waits for one) and so the page comment is reached as the
    // second nav target rather than from idle.
    await createInlineComment(page, ANCHOR_TEXT, "inline anchor");
    // 15 reply rows comfortably exceed the 800px viewport while keeping setup light.
    await createPageCommentWithReplies(page, PAGE_DOC_ID, "tall thread root", 15);
    await reloadIdle(page);

    await page.keyboard.press("n"); // idle → inline (1 of 2)
    await expect(liveRegion(page)).toContainText("Comment 1 of 2");
    await page.keyboard.press("n"); // → page comment (2 of 2)
    await expect(liveRegion(page)).toContainText("Comment 2 of 2");
    // The page comment is not an inline highlight, so nothing is active in-article.
    expect(await activeHighlightId(page)).toBeNull();

    const threadCard = page.getByTestId("comment-thread").filter({ hasText: "tall thread root" });
    // The card is taller than the viewport, so it must really exceed it for this
    // test to exercise top-vs-center alignment at all.
    const cardHeight = await threadCard.evaluate((el) => el.getBoundingClientRect().height);
    expect(cardHeight).toBeGreaterThan(800);

    // The thread card carries the root author/body at its top. With top-alignment
    // the root row is within the viewport; with centering it is scrolled above it.
    await expect(threadCard.getByTestId("comment-avatar-row").first()).toBeInViewport();

    // Tighter guard: the card's top edge is at/below the viewport top (not negative),
    // which is the precise symptom — centering yields a negative top for a tall card.
    const cardTop = await threadCard.evaluate((el) => el.getBoundingClientRect().top);
    expect(cardTop).toBeGreaterThanOrEqual(0);
  });
});
