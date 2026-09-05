import { test, expect } from "@playwright/test";
import {
  createComment,
  fetchComments,
  found,
  resolveAllComments,
  resolveDocumentId,
} from "./comment-helpers";

const DOC_URL = "comment-entity-decoding";
const DOC_PATH = `http://127.0.0.1:8085/${DOC_URL}`;

test("API quote selectors follow browser character-reference decoding", async ({ page }) => {
  await page.goto(DOC_PATH);
  const documentId = await resolveDocumentId(page, DOC_URL);
  await resolveAllComments(page, documentId);

  expect(await page.getByRole("article").textContent()).toContain("before \u0001 suffix");

  const body = `entity-decoding-${Date.now()}`;
  const id = await createComment(page, { documentId, body, quote: "suffix" });
  const created = found(
    (await fetchComments(page, documentId, "open")).find((comment) => comment.id === id),
    "entity-decoding comment",
  );

  expect(created.selectors).toContainEqual({
    type: "TextQuoteSelector",
    exact: "suffix",
    prefix: "before \u0001 ",
    suffix: "\n",
  });
  expect(created.selectors).toContainEqual({
    type: "TextPositionSelector",
    start: 9,
    end: 15,
  });

  await page.reload();
  const annotation = page.locator(`article rw-annotation[data-comment-id="${id}"]`).first();
  await expect(annotation).toBeVisible();
  await annotation.click();
  await expect(page.getByRole("complementary", { name: "Comments" })).toContainText(body);
  await expect(page.getByTestId("orphan-quote")).toHaveCount(0);
});
