import { describe, it, expect, vi, beforeAll } from "vitest";
import { render } from "@testing-library/svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

// CommentThread reads `router` from RwContext; stub it so the component renders
// standalone without mounting the whole App.
vi.mock("$lib/context", () => ({
  getRwContext: () => ({ router: { embedded: true } }),
}));

import CommentThread from "./CommentThread.svelte";
import type { Comment } from "../../types/comments";

beforeAll(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});

function mkComment(over: Partial<Comment> & { id: string }): Comment {
  return {
    documentId: "doc",
    author: { id: "local:human", name: "You" },
    body: "b",
    selectors: [],
    status: "open",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    canDelete: false,
    canRestore: false,
    canResolve: true,
    ...over,
  };
}

const noop = () => {};
const asyncNoop = async () => {};

function renderThread(over: Partial<Comment> & { id: string }) {
  return render(CommentThread, {
    comment: mkComment(over),
    replies: [],
    active: false,
    onResolve: noop,
    onReopen: noop,
    onReply: asyncNoop,
    onDelete: asyncNoop,
    onRestore: asyncNoop,
  });
}

describe("CommentThread canResolve gating", () => {
  it("shows Resolve when canResolve is true", () => {
    const { getByRole } = renderThread({ id: "1", canResolve: true, status: "open" });
    expect(getByRole("button", { name: "Resolve" })).toBeTruthy();
  });

  it("hides Resolve when canResolve is false", () => {
    const { queryByRole } = renderThread({ id: "1", canResolve: false, status: "open" });
    expect(queryByRole("button", { name: "Resolve" })).toBeNull();
  });

  it("hides Reopen when canResolve is false on a resolved comment", () => {
    const { queryByRole } = renderThread({ id: "1", canResolve: false, status: "resolved" });
    expect(queryByRole("button", { name: "Reopen" })).toBeNull();
  });

  it("shows Reopen when canResolve is true on a resolved comment", () => {
    const { getByRole } = renderThread({ id: "1", canResolve: true, status: "resolved" });
    expect(getByRole("button", { name: "Reopen" })).toBeTruthy();
  });
});
