import { test, expect } from "@playwright/test";
import { createComment, resolveAllComments, resolveDocumentId } from "./comment-helpers";

// This spec's own page — see `comment-helpers.ts`.
//
// Distinct from `comment-reply-draft-scope.spec.ts`'s "advanced/plugins" —
// that's the section overview, this page's parent, not this page.
const PAGE_URL = "advanced/plugins/custom-extensions";

test.use({ viewport: { width: 1400, height: 800 } });

/**
 * `page.route()` holds the `list()` response until the new comment has rendered,
 * so it is stale whatever the machine speed or the number of comments.
 *
 * Both comments are asserted because the two failure modes drop different sides:
 * applying the stale response loses the new comment, discarding it loses the
 * pre-existing one. Live reload is off on this port, so the silent-refresh
 * variant belongs to `comment-deleted-row-refresh.spec.ts`.
 */
test("a page comment appears alongside an existing one despite a stale list() response", async ({
  page,
}) => {
  await page.goto(`/${PAGE_URL}`);
  const docId = await resolveDocumentId(page, PAGE_URL);
  await resolveAllComments(page, docId);
  await createComment(page, { documentId: docId, body: "Pre-existing comment", selectors: [] });

  let release = () => {};
  const released = new Promise<void>((resolve) => {
    release = resolve;
  });
  let capturedOnce = false;
  await page.route("**/_api/comments?*", async (route) => {
    if (capturedOnce || route.request().method() !== "GET") {
      await route.continue();
      return;
    }
    capturedOnce = true;
    // route.fetch() sends the request immediately, so this snapshot is taken
    // BEFORE the create() below — that is what makes it stale once released.
    // Delaying before this call instead would let the server see the request
    // after the create, and the response would already contain the new
    // comment — testing nothing.
    const response = await route.fetch();
    await released;
    await route.fulfill({ response });
  });

  const section = page.getByRole("region", { name: "Comments" });
  // `finally`, so a failure below still frees the held GET instead of leaving
  // the route handler waiting on a promise nothing will settle.
  try {
    await page.goto(`/${PAGE_URL}`);
    await page.getByRole("article").waitFor();

    await section.getByPlaceholder("Write a comment...").fill("Newly posted comment");
    await section.getByRole("button", { name: "Comment", exact: true }).click();

    // The held list() GET cannot resolve until `release()` below, so this can
    // only become true once create() has actually appended — never a lucky
    // early poll racing a response that, at this point, is not yet in flight.
    await expect(section).toContainText("Newly posted comment");
  } finally {
    release();
  }

  // Assert the pre-existing comment first: it cannot be on screen until the
  // released list has been applied, so this assertion is itself the wait for
  // that response to have been handled. Checking the new comment first could
  // pass on a poll that ran before any of it happened.
  await expect(section).toContainText("Pre-existing comment");
  await expect(section).toContainText("Newly posted comment");
});
