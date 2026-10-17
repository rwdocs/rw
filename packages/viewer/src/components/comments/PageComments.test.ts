import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import Harness from "../__fixtures__/PageCommentsHarness.svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";
import { Comments } from "../../state/comments.svelte";
import type { CommentApiClient } from "../../api/comments";
import type { Comment } from "../../types/comments";

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function orphan(exact: string): Comment {
  return {
    id: "c1",
    documentId: "doc",
    author: { id: "local:human", name: "You" },
    body: "b",
    selectors: [{ type: "TextQuoteSelector", exact, prefix: "", suffix: "" }],
    status: "open",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    canDelete: false,
    canRestore: false,
    canResolve: true,
  };
}

describe("PageComments quote", () => {
  it("follows a thread's selectors when a refresh replaces the row", async () => {
    const comments = new Comments({} as CommentApiClient, () => {});
    comments.orphanIds = new Set(["c1"]);
    comments.items = [orphan("old passage")];
    const { getByTestId } = render(Harness, { comments });
    expect(getByTestId("orphan-quote").textContent).toContain("old passage");

    comments.items = [orphan("new passage")];
    await tick();

    expect(getByTestId("orphan-quote").textContent).toContain("new passage");
  });
});
