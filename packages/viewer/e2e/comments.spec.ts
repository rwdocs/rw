import { test, expect, Page } from "@playwright/test";

// Wide viewport so the right sidebar (TOC / comments) is visible
test.use({ viewport: { width: 1400, height: 800 } });

// All comment tests share a single SQLite DB and operate on the homepage
// (documentId ""), so running describe blocks in parallel would let one block's
// beforeEach `resolveAllComments` close another block's in-flight comments.
// Serialize everything in this file to keep tests isolated.
test.describe.configure({ mode: "serial" });

/** Select a text range inside the article and trigger the selection popover. */
async function selectText(page: Page, text: string) {
  await page.evaluate((targetText) => {
    const article = document.querySelector("article");
    if (!article) throw new Error("no article");
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const content = walker.currentNode.textContent ?? "";
      const idx = content.indexOf(targetText);
      if (idx === -1) continue;

      const range = document.createRange();
      range.setStart(walker.currentNode, idx);
      range.setEnd(walker.currentNode, idx + targetText.length);
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
      return;
    }
    throw new Error(`text "${targetText}" not found in article`);
  }, text);
}

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

/** Wait for CSS Custom Highlights to be registered (comments loaded and anchored). */
async function waitForHighlights(page: Page) {
  await expect(async () => {
    const has = await page.evaluate(
      () => typeof CSS !== "undefined" && "highlights" in CSS && CSS.highlights.has("rw-comments"),
    );
    expect(has).toBe(true);
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
    const res = await fetch(`/api/comments?documentId=${encodeURIComponent(docId)}`);
    const comments = await res.json();
    for (const c of comments) {
      if (c.status === "open") {
        await fetch(`/api/comments/${c.id}`, {
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
    await resolveAllComments(page, "");
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

    // Re-measure runs on the scroll event; poll until the popover updates.
    await expect
      .poll(async () => (await commentButton.boundingBox())?.y, { timeout: 1000 })
      .not.toBe(before!.y);

    const after = await commentButton.boundingBox();
    expect(after).not.toBeNull();
    // Popover sits in viewport-fixed coords, so a downward scroll of N pushes
    // the highlighted text — and therefore the popover — up by N.
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
    const comments = await page.evaluate(async () => {
      const res = await fetch("/api/comments?documentId=&status=open");
      return res.json();
    });
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

    // Verify via API that the comment has both selector types
    const comments = await page.evaluate(async () => {
      const res = await fetch("/api/comments?documentId=&status=open");
      return res.json();
    });
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
    const comments = await page.evaluate(async () => {
      const res = await fetch("/api/comments?documentId=&status=open");
      return res.json();
    });
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

    const comments = await page.evaluate(async () => {
      const res = await fetch("/api/comments?documentId=&status=open");
      return res.json();
    });
    const persisted = comments.find((c: { body: string }) => c.body === "Persistent comment");
    expect(persisted).toBeTruthy();
    expect(persisted.selectors).toHaveLength(2);
  });
});

test.describe("Page comments", () => {
  test.describe.configure({ mode: "serial" });

  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await resolveAllComments(page, "");
  });

  test("page comments section is visible below article", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Page comments" });
    await expect(section).toBeVisible();
    await expect(section.getByPlaceholder("Write a comment...")).toBeVisible();
  });

  test("creating a page comment via the form", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Page comments" });
    await section.getByPlaceholder("Write a comment...").fill("A page-level comment");
    await section.getByRole("button", { name: "Comment", exact: true }).click();

    // Comment should appear in the section
    await expect(section).toContainText("A page-level comment");
    await expect(section.getByText("You", { exact: true })).toBeVisible();

    // Verify via API — should have no selectors
    const comments = await page.evaluate(async () => {
      const res = await fetch("/api/comments?documentId=&status=open");
      return res.json();
    });
    const created = comments.find((c: { body: string }) => c.body === "A page-level comment");
    expect(created).toBeTruthy();
    expect(created.selectors).toHaveLength(0);
    expect(created.parentId).toBeUndefined();
  });

  test("replying to a page comment", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a page comment
    const section = page.getByRole("region", { name: "Page comments" });
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
    const comments = await page.evaluate(async () => {
      const res = await fetch("/api/comments?documentId=");
      return res.json();
    });
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

  test("resolving a page comment hides it", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("article").waitFor();

    // Create a page comment
    const section = page.getByRole("region", { name: "Page comments" });
    await section.getByPlaceholder("Write a comment...").fill("Comment to resolve");
    await section.getByRole("button", { name: "Comment", exact: true }).click();
    await expect(section).toContainText("Comment to resolve");

    // Resolve it
    await section.getByRole("button", { name: "Resolve", exact: true }).click();

    // Should no longer be visible (resolved threads are hidden)
    await expect(section.getByText("Comment to resolve")).not.toBeVisible();
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

    await page.evaluate(async () => {
      await fetch("/api/comments", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          documentId: "",
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
    });

    // Reload so the viewer picks up the new comment via its load path.
    await page.goto("/");
    await page.getByRole("article").waitFor();

    const section = page.getByRole("region", { name: "Page comments" });
    await expect(section).toContainText("This paragraph needs a rewrite");

    // Quote context is shown with the stored exact text and its surrounding
    // prefix/suffix, so reviewers can still tell what the comment referred to.
    const quote = section.getByTestId("orphan-quote");
    await expect(quote).toBeVisible();
    await expect(quote).toContainText("context before");
    await expect(quote).toContainText("a sentence that definitely is not on this page");
    await expect(quote).toContainText("context after");

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
    await resolveAllComments(page, "");
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
      const highlights = CSS.highlights as unknown as Map<string, Highlight>;
      const highlight = highlights.get("rw-comment-active");
      if (!highlight) throw new Error("no active highlight");
      const range = [...highlight][0] as Range;
      const firstLine = range.getClientRects()[0] ?? range.getBoundingClientRect();
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
      const highlights = CSS.highlights as unknown as Map<string, Highlight>;
      const highlight = highlights.get("rw-comment-active");
      if (!highlight) throw new Error("no active highlight");
      const range = [...highlight][0] as Range;
      const firstLine = range.getClientRects()[0] ?? range.getBoundingClientRect();
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
