// This test is intentionally read-only and therefore safe to run in parallel
// with comments.spec.ts without a serial guard: the POST is stubbed to 503 so
// no comment is ever written to the shared SQLite DB; GETs fall through to the
// real server.
// Do NOT add a test here that successfully creates a comment — it would write
// to the shared DB and could race comments.spec.ts.
import { test, expect } from "@playwright/test";
import { SAVE_FAILED_MESSAGE } from "../src/lib/comments/messages";

// Wide viewport so the page comment form is comfortably visible.
test.use({ viewport: { width: 1400, height: 800 } });

test("keeps the draft and surfaces a toast + Retry when the save fails", async ({ page }) => {
  // Fail comment creation (POST) but let the GET list through, so the page
  // renders normally and only the save fails.
  await page.route("**/_api/comments", async (route) => {
    if (route.request().method() === "POST") {
      await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
    } else {
      await route.fallback();
    }
  });

  await page.goto("/");

  const draft = "This is a carefully written draft I do not want to lose.";
  const textarea = page.getByPlaceholder("Write a comment...");
  await textarea.fill(draft);
  await textarea.press("Meta+Enter");

  // The save failed: the button offers Retry, the draft is preserved, and the
  // toast explains what happened and that the draft is safe.
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  await expect(textarea).toHaveValue(draft);
  await expect(page.getByText(SAVE_FAILED_MESSAGE)).toBeVisible();
});
