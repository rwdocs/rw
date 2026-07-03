import { test, expect, Page } from "@playwright/test";
import { resolveDocumentId, selectText } from "./comment-helpers";

// Wide viewport so the right sidebar (TOC / comments) is visible
test.use({ viewport: { width: 1400, height: 800 } });

// All comment tests share a single SQLite DB and operate on the homepage
// (documentId ""), so running describe blocks in parallel would let one block's
// beforeEach `resolveAllComments` close another block's in-flight comments.
// Serialize everything in this file to keep tests isolated.
test.describe.configure({ mode: "serial" });

/** Activate comments sidebar by clicking the highlighted text. */
async function clickHighlight(page: Page, text: string) {
  // Wait for highlights to be registered first
  await waitForHighlights(page);

  // Scroll the target text into view — comment creation may have scrolled the page
  await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const content = walker.currentNode.textContent ?? "";
      const idx = content.indexOf(targetText);
      if (idx === -1) continue;
      const node = walker.currentNode;
      const range = document.createRange();
      range.setStart(node, idx);
      range.setEnd(node, idx + targetText.length);
      // Scroll into viewport center
      const el = range.startContainer.parentElement;
      el?.scrollIntoView({ block: "center" });
      return;
    }
    throw new Error(`text "${targetText}" not found`);
  }, text);

  // Small delay for scroll to settle, then get viewport coordinates
  await page.waitForTimeout(100);

  const clickCoords = await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const content = walker.currentNode.textContent ?? "";
      const idx = content.indexOf(targetText);
      if (idx === -1) continue;
      const range = document.createRange();
      range.setStart(walker.currentNode, idx + 1);
      range.setEnd(walker.currentNode, idx + 2);
      const rect = range.getBoundingClientRect();
      return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    }
    throw new Error(`text "${targetText}" not found`);
  }, text);

  await page.mouse.click(clickCoords.x, clickCoords.y);
}

/** Wait for comment wrappers to be present in the article (comments loaded and anchored). */
async function waitForHighlights(page: Page) {
  await expect(async () => {
    const count = await page.evaluate(
      () => document.querySelectorAll("article rw-annotation").length,
    );
    expect(count).toBeGreaterThan(0);
  }).toPass({ timeout: 10000 });
}

/** Create a comment on the given text via the UI (select + popover + form). */
async function createCommentViaUI(page: Page, targetText: string, body: string) {
  await selectText(page, targetText);
  await page.getByRole("button", { name: "Add comment" }).click();
  const sidebar = page.getByRole("complementary", { name: "Comments" });
  await sidebar.getByPlaceholder("Write a comment...").fill(body);
  await sidebar.getByRole("button", { name: "Comment", exact: true }).click();
  // Wait for form to close
  await expect(sidebar.getByPlaceholder("Write a comment...")).not.toBeVisible();
}

/** Resolve all comments for a document via the API so they don't interfere with new tests. */
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

test.describe("Inline comments", () => {
  // Tests share a single SQLite DB, so run serially to avoid conflicts
  test.describe.configure({ mode: "serial" });

  test.beforeEach(async ({ page }) => {
    // Resolve all comments from previous test runs so they don't interfere
    await page.goto("/");
    const docId = await resolveDocumentId(page, "");
    await resolveAllComments(page, docId);
  });

  test("selecting text shows the comment popover", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectText(page, "Navigation sidebar");

    const commentButton = page.getByRole("button", { name: "Add comment" });
    await expect(commentButton).toBeVisible();
  });

  test("popover follows the highlighted text on scroll", async ({ page }) => {
    // Shrink the viewport so the homepage overflows and the window scrolls.
    await page.setViewportSize({ width: 1400, height: 400 });
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectText(page, "Navigation sidebar");

    const commentButton = page.getByRole("button", { name: "Add comment" });
    await expect(commentButton).toBeVisible();

    const before = await commentButton.boundingBox();
    expect(before).not.toBeNull();

    const scrollDelta = 50;
    await page.evaluate((y) => window.scrollBy(0, y), scrollDelta);

    await expect
      .poll(async () => (await commentButton.boundingBox())?.y, { timeout: 1000 })
      .not.toBe(before!.y);

    const after = await commentButton.boundingBox();
    expect(after).not.toBeNull();
    // The popover shares the article's scroll layer, so a downward scroll of N
    // moves the highlighted text — and the popover with it — up by N.
    expect(after!.y).toBeCloseTo(before!.y - scrollDelta, 0);
  });

  test("clicking the selected text dismisses the comment popover", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectText(page, "Navigation sidebar");

    const commentButton = page.getByRole("button", { name: "Add comment" });
    await expect(commentButton).toBeVisible();

    // Real mouse click — Playwright's page.mouse fires native events so the
    // browser performs its default click-on-selection collapse. The synthetic
    // event used by selectText() can't reproduce that.
    const clickPoint = await page.evaluate((targetText) => {
      const article = document.querySelector("article")!;
      const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) {
        const idx = (walker.currentNode.textContent ?? "").indexOf(targetText);
        if (idx === -1) continue;
        const range = document.createRange();
        range.setStart(walker.currentNode, idx);
        range.setEnd(walker.currentNode, idx + targetText.length);
        const rect = range.getBoundingClientRect();
        return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      }
      throw new Error(`text "${targetText}" not found`);
    }, "Navigation sidebar");

    await page.mouse.click(clickPoint.x, clickPoint.y);

    await expect(commentButton).not.toBeVisible();
  });

  test("creating a comment via the popover", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Select text and open the comment form
    await selectText(page, "code highlighting");
    await page.getByRole("button", { name: "Add comment" }).click();

    // Fill in and submit the comment
    const sidebar = page.getByRole("complementary", { name: "Comments" });
    const textarea = sidebar.getByPlaceholder("Write a comment...");
    await expect(textarea).toBeVisible();
    await textarea.fill("Needs more detail on syntax highlighting");
    await sidebar.getByRole("button", { name: "Comment", exact: true }).click();

    // Form should disappear
    await expect(textarea).not.toBeVisible();

    // Thread card shows the server-stamped author
    await expect(sidebar.getByText("You", { exact: true })).toBeVisible();

    // Comment should be persisted via the API
    const docId = await resolveDocumentId(page, "");
    const comments = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}&status=open`);
      return res.json();
    }, docId);
    const created = comments.find(
      (c: { body: string }) => c.body === "Needs more detail on syntax highlighting",
    );
    expect(created).toBeTruthy();
    expect(created.selectors).toHaveLength(2);
  });

  test("comment created via UI has correct selectors and is retrievable", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a comment via the UI
    await selectText(page, "code highlighting");
    await page.getByRole("button", { name: "Add comment" }).click();
    const sidebar = page.getByRole("complementary", { name: "Comments" });
    await sidebar.getByPlaceholder("Write a comment...").fill("Check highlight");
    await sidebar.getByRole("button", { name: "Comment", exact: true }).click();
    // Wait for the form to close — confirms the POST completed before we query.
    await expect(sidebar.getByPlaceholder("Write a comment...")).not.toBeVisible();

    // Verify via API that the comment has both selector types
    const docId = await resolveDocumentId(page, "");
    const comments = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}&status=open`);
      return res.json();
    }, docId);
    const created = comments.find((c: { body: string }) => c.body === "Check highlight");
    expect(created).toBeTruthy();
    const types = created.selectors.map((s: { type: string }) => s.type);
    expect(types).toContain("TextQuoteSelector");
    expect(types).toContain("TextPositionSelector");

    // Verify the TextQuoteSelector captured the right text
    const quote = created.selectors.find((s: { type: string }) => s.type === "TextQuoteSelector");
    expect(quote.exact).toBe("code highlighting");
  });

  test("clicking a highlight replaces TOC with comments sidebar", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // TOC should be visible initially
    const tocSidebar = page.getByRole("complementary", { name: "Page outline" });
    await expect(tocSidebar).toBeVisible();

    // Create a comment via UI. Creation activates the new thread, so dismiss
    // it via the close button to restore the TOC before we test click-to-activate.
    await createCommentViaUI(page, "code highlighting", "Review this section");
    const commentsSidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(commentsSidebar).toBeVisible();
    await commentsSidebar.getByRole("button", { name: "Close comment", exact: true }).click();
    await expect(tocSidebar).toBeVisible();

    // Click on the highlighted text
    await clickHighlight(page, "code highlighting");

    // Comments sidebar should replace TOC
    await expect(commentsSidebar).toBeVisible();
    await expect(commentsSidebar).toContainText("Review this section");

    // TOC should be gone
    await expect(tocSidebar).not.toBeVisible();
  });

  test("close button dismisses comments and restores TOC", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // createCommentViaUI leaves the new thread active — sidebar is already visible.
    await createCommentViaUI(page, "code highlighting", "Some comment");
    const commentsSidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(commentsSidebar).toBeVisible();

    // Click the close button on the comment card
    await commentsSidebar.getByRole("button", { name: "Close comment", exact: true }).click();

    // TOC should be restored
    const tocSidebar = page.getByRole("complementary", { name: "Page outline" });
    await expect(tocSidebar).toBeVisible();
    await expect(commentsSidebar).not.toBeVisible();
  });

  test("resolving a comment updates its status", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // createCommentViaUI leaves the new thread active — sidebar is already visible.
    await createCommentViaUI(page, "code highlighting", "Fix this text");
    const commentsSidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(commentsSidebar).toBeVisible();

    // Click Resolve
    await commentsSidebar.getByRole("button", { name: "Resolve", exact: true }).click();

    // Should now show Reopen instead of Resolve
    await expect(
      commentsSidebar.getByRole("button", { name: "Reopen", exact: true }),
    ).toBeVisible();
  });

  test("cancel button dismisses the comment form", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectText(page, "Navigation sidebar");
    await page.getByRole("button", { name: "Add comment" }).click();

    const sidebar = page.getByRole("complementary", { name: "Comments" });
    const textarea = sidebar.getByPlaceholder("Write a comment...");
    await expect(textarea).toBeVisible();

    // Focus the textarea so the Cancel/Comment buttons render (the form collapses
    // those actions when idle; autofocus doesn't fire reliably in headless).
    await textarea.focus();
    await sidebar.getByRole("button", { name: "Cancel" }).click();
    await expect(textarea).not.toBeVisible();
  });

  test("replying to a comment shows the reply in the thread", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a top-level comment — leaves the new thread active, sidebar visible.
    await createCommentViaUI(page, "code highlighting", "Parent comment");
    const commentsSidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(commentsSidebar).toContainText("Parent comment");

    // Submit a reply
    const replyTextarea = commentsSidebar.getByPlaceholder("Write a reply...");
    await replyTextarea.fill("Reply to parent");
    await replyTextarea.press("Meta+Enter");

    // Reply should appear in the thread
    await expect(commentsSidebar).toContainText("Reply to parent");

    // Verify via API that the reply has the correct parentId
    const docId = await resolveDocumentId(page, "");
    const comments = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}&status=open`);
      return res.json();
    }, docId);
    const parent = comments.find((c: { body: string }) => c.body === "Parent comment");
    const reply = comments.find((c: { body: string }) => c.body === "Reply to parent");
    expect(parent).toBeTruthy();
    expect(reply).toBeTruthy();
    expect(reply.parentId).toBe(parent.id);
  });

  test("comments persist in the database", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // createCommentViaUI leaves the new thread active — sidebar is already visible.
    await createCommentViaUI(page, "code highlighting", "Persistent comment");
    await expect(page.getByRole("complementary", { name: "Comments" })).toContainText(
      "Persistent comment",
    );

    // Verify the comment survives in the database after a full page load
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const docId = await resolveDocumentId(page, "");
    const comments = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}&status=open`);
      return res.json();
    }, docId);
    const persisted = comments.find((c: { body: string }) => c.body === "Persistent comment");
    expect(persisted).toBeTruthy();
    expect(persisted.selectors).toHaveLength(2);
  });

  test("top-level comments never show a Delete button", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();
    await createCommentViaUI(page, "code highlighting", "Top-level only resolves");

    const sidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(sidebar.getByText("Top-level only resolves")).toBeVisible();

    // Top-level uses Resolve (not Delete) in this model.
    await expect(sidebar.getByRole("button", { name: "Resolve", exact: true })).toBeVisible();
    await expect(sidebar.getByRole("button", { name: "Delete", exact: true })).toHaveCount(0);
  });

  test("delete a reply and restore in session", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create parent + a reply
    await createCommentViaUI(page, "code highlighting", "Parent thread");
    const sidebar = page.getByRole("complementary", { name: "Comments" });
    const replyTextarea = sidebar.getByPlaceholder("Write a reply...");
    await replyTextarea.fill("Mistaken reply");
    await replyTextarea.press("Meta+Enter");
    await expect(sidebar.getByText("Mistaken reply")).toBeVisible();

    // Only the reply has a Delete button (top-level is never deletable).
    await expect(sidebar.getByRole("button", { name: "Delete", exact: true })).toHaveCount(1);
    await sidebar.getByRole("button", { name: "Delete", exact: true }).click();

    // Deleted reply swaps Delete for Restore.
    await expect(sidebar.getByRole("button", { name: "Restore", exact: true })).toHaveCount(1);
    await expect(sidebar.getByRole("button", { name: "Delete", exact: true })).toHaveCount(0);

    // Restore — Delete returns, Restore disappears.
    await sidebar.getByRole("button", { name: "Restore", exact: true }).click();
    await expect(sidebar.getByRole("button", { name: "Delete", exact: true })).toHaveCount(1);
    await expect(sidebar.getByRole("button", { name: "Restore", exact: true })).toHaveCount(0);
  });

  test("delete a reply and reload — reply is gone", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();
    await createCommentViaUI(page, "code highlighting", "Parent of vanishing reply");

    const sidebar = page.getByRole("complementary", { name: "Comments" });
    const replyTextarea = sidebar.getByPlaceholder("Write a reply...");
    await replyTextarea.fill("Vanish after reload");
    await replyTextarea.press("Meta+Enter");
    await expect(sidebar.getByText("Vanish after reload")).toBeVisible();

    await sidebar.getByRole("button", { name: "Delete", exact: true }).click();
    // Confirm the soft-delete landed: Restore button now exists in the row.
    await expect(sidebar.getByRole("button", { name: "Restore", exact: true })).toHaveCount(1);

    // Reload — the server filters out deleted comments by default, so the
    // reply body should be gone after refetch.
    await page.reload();
    await page.getByRole("article").waitFor();

    await expect(page.getByText("Vanish after reload")).toBeHidden();
    // The API should not return the deleted reply either.
    const docId = await resolveDocumentId(page, "");
    const list = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}`);
      return res.json();
    }, docId);
    const found = (list as { body: string }[]).find((c) => c.body === "Vanish after reload");
    expect(found).toBeUndefined();
  });

  test("overlapping comments render with nested wrappers and translucent backgrounds", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Text under test is the homepage sentence:
    //   "Welcome to the test documentation site."
    // Outer covers [Welcome..documentation], inner covers [to..site] — they
    // overlap on "to the test documentation".
    await createCommentViaUI(page, "Welcome to the test documentation", "outer");
    await waitForHighlights(page);
    await createCommentViaUI(page, "to the test documentation site", "inner");
    // Wait for the *nested* wrapper to exist — `waitForHighlights` only checks
    // that any wrapper is present, which is already true from the outer comment.
    // Without this, the depth-2 assertion below races re-anchoring.
    await expect(async () => {
      const hasNested = await page.evaluate(
        () => document.querySelector("article rw-annotation rw-annotation") !== null,
      );
      expect(hasNested).toBe(true);
    }).toPass({ timeout: 10000 });

    // For each substring, find the deepest <rw-annotation> whose textContent
    // includes it, and report its nesting depth + computed background color.
    // We search WRAPPERS rather than text nodes because wrapping splits text
    // nodes at range boundaries — a substring like "documentation" may be in
    // its own text node now, but the wrapper around it tells us the depth.
    const samples = await page.evaluate(() => {
      function depthOf(el: Element): number {
        let depth = 0;
        let cur: Node | null = el;
        while (cur) {
          if (cur instanceof Element && cur.tagName.toLowerCase() === "rw-annotation") {
            depth++;
          }
          cur = cur.parentNode;
        }
        return depth;
      }
      function deepestWrapperContaining(substring: string) {
        const all = Array.from(document.querySelectorAll("article rw-annotation"));
        let best: { el: Element; depth: number } | null = null;
        for (const el of all) {
          if (!(el.textContent ?? "").includes(substring)) continue;
          const depth = depthOf(el);
          if (!best || depth > best.depth) best = { el, depth };
        }
        if (!best) return null;
        return {
          depth: best.depth,
          background: window.getComputedStyle(best.el).backgroundColor,
        };
      }
      return {
        // "Welcome" sits in a text node inside the outer wrapper only.
        outerOnly: deepestWrapperContaining("Welcome"),
        // "the test" sits inside the inner wrapper which is nested inside
        // the outer wrapper — depth 2.
        overlap: deepestWrapperContaining("the test"),
        // "site" sits in a text node inside the inner wrapper only.
        innerOnly: deepestWrapperContaining("site"),
      };
    });

    expect(samples.outerOnly?.depth).toBe(1);
    expect(samples.overlap?.depth).toBe(2);
    expect(samples.innerOnly?.depth).toBe(1);

    // All wrappers must use a translucent background so nested wrappers
    // composite to a darker yellow. If a future CSS change makes the rule
    // opaque (alpha >= 1) or transparent (alpha == 0), the visible "darker
    // overlap" feature regresses.
    function alphaOf(color: string): number {
      // Modern Chrome serializes color-mix() results in the CSS Color 4
      // `color(srgb r g b / a)` form. Legacy browsers (and color values that
      // resolve to legacy spaces) serialize as `rgb(r, g, b)` (implicit
      // alpha 1) or `rgba(r, g, b, a)`. Parse both shapes.
      if (color.startsWith("color(")) {
        const m = color.match(/\/\s*([\d.]+)\s*\)/);
        return m ? parseFloat(m[1]) : 1;
      }
      const m = color.match(/rgba?\(([^)]+)\)/);
      if (!m) throw new Error(`unrecognized color "${color}"`);
      const parts = m[1].split(",").map((p) => parseFloat(p.trim()));
      return parts.length === 4 ? parts[3] : 1;
    }
    const outerAlpha = alphaOf(samples.outerOnly!.background);
    const overlapAlpha = alphaOf(samples.overlap!.background);
    expect(outerAlpha).toBeGreaterThan(0);
    expect(outerAlpha).toBeLessThan(1);
    expect(overlapAlpha).toBeGreaterThan(0);
    expect(overlapAlpha).toBeLessThan(1);
  });
});

test.describe("Page comments", () => {
  test.describe.configure({ mode: "serial" });

  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    const docId = await resolveDocumentId(page, "");
    await resolveAllComments(page, docId);
  });

  test("page comments section is visible below article", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await expect(section).toBeVisible();
    await expect(section.getByPlaceholder("Write a comment...")).toBeVisible();
  });

  test("page comments section shows a Comments heading", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await expect(section.getByRole("heading", { name: "Comments" })).toBeVisible();

    // beforeEach resolves all comments, so the section starts empty:
    // the count badge is hidden when there are no visible threads.
    await expect(section.getByLabel(/^\d+ comments?$/)).toHaveCount(0);
  });

  test("comment count badge reflects the number of page comments", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill("First counted comment");
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText("First counted comment");

    const countBadge = section.getByLabel(/^\d+ comments?$/);
    await expect(countBadge).toHaveText("1");
    await expect(countBadge).toHaveAccessibleName("1 comment");

    // A second comment proves the count is dynamic and exercises the plural
    // label branch ("comments" vs the singular "comment" above).
    await section.getByPlaceholder("Write a comment...").fill("Second counted comment");
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText("Second counted comment");

    await expect(countBadge).toHaveText("2");
    await expect(countBadge).toHaveAccessibleName("2 comments");
  });

  test("creating a page comment via the form", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill("A page-level comment");
    await section.getByRole("button", { name: "Comment", exact: true }).click();

    // Comment should appear in the section
    await expect(section).toContainText("A page-level comment");
    await expect(section.getByText("You", { exact: true })).toBeVisible();

    // Verify via API — should have no selectors
    const docId = await resolveDocumentId(page, "");
    const comments = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}&status=open`);
      return res.json();
    }, docId);
    const created = comments.find((c: { body: string }) => c.body === "A page-level comment");
    expect(created).toBeTruthy();
    expect(created.selectors).toHaveLength(0);
    expect(created.parentId).toBeUndefined();
  });

  test("replying to a page comment", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a page comment
    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill("Top-level page comment");
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText("Top-level page comment");

    // Reply to it
    const replyInput = section.getByPlaceholder("Write a reply...");
    await replyInput.fill("Reply to page comment");
    await replyInput.press("Meta+Enter");

    // Reply should appear
    await expect(section).toContainText("Reply to page comment");

    // Verify via API — find the reply and check its parentId
    const docId = await resolveDocumentId(page, "");
    const comments = await page.evaluate(async (docId) => {
      const res = await fetch(`/_api/comments?documentId=${encodeURIComponent(docId)}`);
      return res.json();
    }, docId);
    const reply = comments.find(
      (c: { body: string; parentId?: string }) => c.body === "Reply to page comment" && c.parentId,
    );
    expect(reply).toBeTruthy();
    // The parent should exist and be a page comment (no selectors)
    const parent = comments.find((c: { id: string }) => c.id === reply.parentId);
    expect(parent).toBeTruthy();
    expect(parent.body).toBe("Top-level page comment");
    expect(parent.selectors).toHaveLength(0);
  });

  test("comment body renders markdown paragraphs", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a comment whose body has two blank-line-separated paragraphs.
    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill("First para.\n\nSecond para.");
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText("First para.");

    const body = section.getByTestId("comment-body");
    // Two <p> elements render (the run-on bug is fixed), not one text node.
    await expect(body.locator("p")).toHaveCount(2);
    await expect(body).toContainText("First para.");
    await expect(body).toContainText("Second para.");
  });

  test("resolving a page comment hides it", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a page comment
    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill("Comment to resolve");
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText("Comment to resolve");

    // Resolve it
    await section.getByRole("button", { name: "Resolve", exact: true }).click();

    // Should no longer be visible (resolved threads are hidden)
    await expect(section.getByText("Comment to resolve")).not.toBeVisible();
  });

  test("resolved page comment can be revealed via the Show resolved toggle", async ({ page }) => {
    const unique = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    const body = `Resolved then revealed ${unique}`;

    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill(body);
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText(body);

    // Resolve it — it leaves the open list.
    await section.getByRole("button", { name: "Resolve", exact: true }).click();
    await expect(section.getByText(body)).toBeHidden();

    // The disclosure offers to reveal the resolved comments, collapsed by default.
    const toggle = section.getByRole("button", { name: /Show resolved\s*\d+/ });
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");

    // Expanding reveals the resolved comment and flips the label.
    await toggle.click();
    await expect(section.getByRole("button", { name: "Hide resolved" })).toBeVisible();
    await expect(section.getByText(body)).toBeVisible();
  });

  test("reopening from the resolved list returns the comment to the open list", async ({
    page,
  }) => {
    const unique = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    const body = `Reopen me from resolved ${unique}`;

    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await section.getByPlaceholder("Write a comment...").fill(body);
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText(body);
    await section.getByRole("button", { name: "Resolve", exact: true }).click();
    await expect(section.getByText(body)).toBeHidden();

    // Reveal the resolved list and reopen the specific thread.
    await section.getByRole("button", { name: /Show resolved\s*\d+/ }).click();
    const thread = section.getByTestId("comment-thread").filter({ hasText: body });
    await thread.getByRole("button", { name: "Reopen", exact: true }).click();

    // It returns to the open list: the thread now renders in its open state,
    // offering Resolve (not Reopen). This proves the status actually flipped and
    // the thread left the resolved disclosure — not merely that its text is still
    // visible in the (still-expanded) resolved list.
    const reopened = section.getByTestId("comment-thread").filter({ hasText: body });
    await expect(reopened).toBeVisible();
    await expect(reopened.getByRole("button", { name: "Resolve", exact: true })).toBeVisible();
    await expect(reopened.getByRole("button", { name: "Reopen", exact: true })).toHaveCount(0);
  });

  test("no resolved comments means no Show resolved toggle", async ({ page }) => {
    // A sub-page that no other test posts comments to, so it has zero resolved.
    await page.goto("/api/endpoints");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await expect(section).toBeVisible();
    await expect(section.getByRole("button", { name: /Show resolved/ })).toHaveCount(0);
    await expect(section.getByRole("button", { name: "Hide resolved" })).toHaveCount(0);
  });

  test("resolved comment quote uses a neutral, accurate tooltip", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });

    // Create an inline-anchored comment via the API whose passage still exists,
    // then resolve it, so it appears in the resolved list with a quote.
    // Use a unique body to avoid false matches from prior test runs.
    // "code highlighting" appears in the fixture index.md bullet list and is
    // verified anchorable by the existing UI tests that select it directly.
    const unique = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    const body = `Anchored and resolved ${unique}`;
    const docId = await resolveDocumentId(page, "");
    await page.evaluate(
      async ({ commentBody, docId }) => {
        const create = await fetch("/_api/comments", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            documentId: docId,
            body: commentBody,
            quote: "code highlighting",
          }),
        });
        const created = await create.json();
        await fetch(`/_api/comments/${created.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ status: "resolved" }),
        });
      },
      { commentBody: body, docId },
    );
    await page.reload();
    await page.getByRole("article").waitFor();

    await section.getByRole("button", { name: /Show resolved\s*\d+/ }).click();
    const thread = section.getByTestId("comment-thread").filter({ hasText: body });
    const quote = thread.getByTestId("orphan-quote");
    await expect(quote).toBeVisible();
    await expect(quote).toHaveAttribute("title", "The passage this comment was attached to.");
  });

  test("resolved inline comment leaves no highlight in the article", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Anchor a comment to live article text, then resolve it. Per the design,
    // resolved comments surface only in the bottom page-comments block — never as
    // an article highlight. The beforeEach resolves all comments first, so the
    // article starts with zero highlights; this comment must not add one.
    const unique = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    const body = `Resolved no highlight ${unique}`;
    const docId = await resolveDocumentId(page, "");
    await page.evaluate(
      async ({ commentBody, docId }) => {
        const create = await fetch("/_api/comments", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            documentId: docId,
            body: commentBody,
            quote: "code highlighting",
          }),
        });
        const created = await create.json();
        await fetch(`/_api/comments/${created.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ status: "resolved" }),
        });
      },
      { commentBody: body, docId },
    );
    await page.reload();
    await page.getByRole("article").waitFor();

    // The resolved comment is reachable in the bottom block...
    const section = page.getByRole("region", { name: "Comments" });
    await section.getByRole("button", { name: /Show resolved\s*\d+/ }).click();
    await expect(section.getByTestId("comment-thread").filter({ hasText: body })).toBeVisible();

    // ...but its passage carries no <rw-annotation> highlight in the article body.
    await expect(page.getByRole("article").locator("rw-annotation")).toHaveCount(0);
  });

  test("inline comment whose passage is gone surfaces in the page comments timeline", async ({
    page,
  }) => {
    // Simulate a comment whose stored passage no longer exists in the doc by
    // POSTing explicit selectors with an `exact` string that doesn't match any
    // text on the page. This is what live-reload after a content edit looks
    // like to the viewer: stored selectors that can't be re-anchored.
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const docId = await resolveDocumentId(page, "");
    await page.evaluate(async (docId) => {
      await fetch("/_api/comments", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          documentId: docId,
          body: "This paragraph needs a rewrite",
          selectors: [
            {
              type: "TextQuoteSelector",
              exact: "a sentence that definitely is not on this page",
              prefix: "context before ",
              suffix: " context after",
            },
          ],
        }),
      });
    }, docId);

    // Reload so the viewer picks up the new comment via its load path.
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Comments" });
    await expect(section).toContainText("This paragraph needs a rewrite");

    // Quote context is shown with the stored exact text and its surrounding
    // prefix/suffix, so reviewers can still tell what the comment referred to.
    const quote = section.getByTestId("orphan-quote");
    await expect(quote).toBeVisible();
    await expect(quote).toContainText("context before");
    await expect(quote).toContainText("a sentence that definitely is not on this page");
    await expect(quote).toContainText("context after");

    // The quote sits inside the comment card, between the author row and the
    // body — scoped to the card, not the page-comments section.
    const card = section.getByTestId("comment-thread");
    await expect(card).toBeVisible();
    await expect(card.getByTestId("orphan-quote")).toBeVisible();
    await expect(card).toContainText("This paragraph needs a rewrite");

    // The quote must come before the comment body in document order, so it
    // reads as the opening of the first comment message, not a trailer.
    const quoteBeforeBody = await card.evaluate((cardEl) => {
      const q = cardEl.querySelector('[data-testid="orphan-quote"]');
      const body = cardEl.querySelector('[data-testid="comment-body"]');
      if (!q || !body) {
        throw new Error("expected both orphan-quote and comment-body inside the card");
      }
      return Boolean(q.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING);
    });
    expect(quoteBeforeBody).toBe(true);

    // The orphan must not inflate the inline sidebar's prev/next counter.
    // Create a real inline comment and check that the sidebar counter is 1/1.
    await createCommentViaUI(page, "code highlighting", "Real inline");
    const sidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(sidebar).toBeVisible();
    // nav strip only renders when there are 2+ inline threads, so its absence
    // is the signal that the orphan was excluded from inline navigation.
    await expect(sidebar.getByRole("button", { name: "Previous comment" })).toHaveCount(0);
    await expect(sidebar.getByRole("button", { name: "Next comment" })).toHaveCount(0);
  });
});

test.describe("Comment anchor alignment", () => {
  // Tests share a single SQLite DB, so run serially to avoid conflicts
  test.describe.configure({ mode: "serial" });

  test.beforeEach(async ({ page }) => {
    // Resolve any open comments from previous tests
    await page.goto("/");
    const docId = await resolveDocumentId(page, "");
    await resolveAllComments(page, docId);
  });

  test("activated thread: avatar row vertical center aligns with highlight first-line center", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Seed an inline comment — createCommentViaUI leaves it active afterward,
    // so the active-highlight is already set when we measure below.
    await createCommentViaUI(page, "code highlighting", "Alignment check");

    // Confirm the comments sidebar is showing an active thread
    const sidebar = page.getByRole("complementary", { name: "Comments" });
    await expect(sidebar).toBeVisible();
    await expect(sidebar.getByTestId("comment-avatar-row").first()).toBeVisible();

    const highlightMiddle = await page.evaluate(() => {
      const wrapper = document.querySelector('article rw-annotation[data-active="true"]');
      if (!wrapper) throw new Error("no active wrapper");
      const firstLine = wrapper.getClientRects()[0] ?? wrapper.getBoundingClientRect();
      return firstLine.top + firstLine.height / 2;
    });

    const rowBox = await page.getByTestId("comment-avatar-row").first().boundingBox();
    if (!rowBox) throw new Error("no avatar row bbox");
    const center = rowBox.y + rowBox.height / 2;

    expect(Math.abs(center - highlightMiddle)).toBeLessThanOrEqual(2);
  });

  test("two threads: alignment holds after creating a second comment (nav variant switch)", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Seed a first inline comment on "code highlighting" — leaves it active afterward
    await createCommentViaUI(page, "code highlighting", "first");

    // Dismiss the active thread so we can start a fresh selection
    await page
      .getByRole("complementary", { name: "Comments" })
      .getByRole("button", {
        name: "Close comment",
        exact: true,
      })
      .click();

    // Create a second comment on a different phrase — the first thread now gains
    // a prev/next nav strip (2 inline threads total).
    await createCommentViaUI(page, "Navigation sidebar", "second");

    // Activate the first comment — its header now has the nav variant.
    await clickHighlight(page, "code highlighting");

    const highlightMiddle = await page.evaluate(() => {
      const wrapper = document.querySelector('article rw-annotation[data-active="true"]');
      if (!wrapper) throw new Error("no active wrapper");
      const firstLine = wrapper.getClientRects()[0] ?? wrapper.getBoundingClientRect();
      return firstLine.top + firstLine.height / 2;
    });

    const rowBox = await page.getByTestId("comment-avatar-row").first().boundingBox();
    if (!rowBox) throw new Error("no row bbox after second comment");
    const center = rowBox.y + rowBox.height / 2;

    expect(Math.abs(center - highlightMiddle)).toBeLessThanOrEqual(2);
  });

  test("pending form: textarea content-box top aligns with selection top", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    await selectText(page, "Navigation sidebar");
    const selectionTop = await page.evaluate(() => {
      const sel = window.getSelection();
      if (!sel || sel.rangeCount === 0) throw new Error("no selection");
      return sel.getRangeAt(0).getBoundingClientRect().top;
    });
    await page.getByRole("button", { name: /add comment/i }).click();

    const textarea = page.getByRole("complementary", { name: "Comments" }).getByRole("textbox");
    await textarea.waitFor({ state: "visible" });

    const offset = await textarea.evaluate((ta: HTMLTextAreaElement) => {
      const rect = ta.getBoundingClientRect();
      const padding = parseFloat(getComputedStyle(ta).paddingTop) || 0;
      return rect.top + padding;
    });

    expect(Math.abs(offset - selectionTop)).toBeLessThanOrEqual(2);
  });
});
