import { describe, it, expect, vi, beforeAll } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

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

describe("CommentThread reply draft", () => {
  it("seeds the reply form from initialReplyDraft", () => {
    const { getByPlaceholderText } = render(CommentThread, {
      comment: mkComment({ id: "1", status: "open" }),
      replies: [],
      active: true,
      onResolve: noop,
      onReopen: noop,
      onReply: asyncNoop,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
      initialReplyDraft: "half-written reply",
    });
    const ta = getByPlaceholderText("Write a reply...") as HTMLTextAreaElement;
    expect(ta.value).toBe("half-written reply");
  });

  it("reports draft edits via onReplyDraftChange keyed by thread id", async () => {
    const onReplyDraftChange = vi.fn();
    const { getByPlaceholderText } = render(CommentThread, {
      comment: mkComment({ id: "thread-7", status: "open" }),
      replies: [],
      active: true,
      onResolve: noop,
      onReopen: noop,
      onReply: asyncNoop,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
      onReplyDraftChange,
    });
    const ta = getByPlaceholderText("Write a reply...") as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: "typing a reply" } });
    expect(onReplyDraftChange).toHaveBeenCalledWith("thread-7", "typing a reply");
  });

  it("reports an empty draft once the reply is submitted, clearing the stored draft", async () => {
    const onReplyDraftChange = vi.fn();
    const onReply = vi.fn().mockResolvedValue(undefined);
    const { getByPlaceholderText } = render(CommentThread, {
      comment: mkComment({ id: "thread-9", status: "open" }),
      replies: [],
      active: true,
      onResolve: noop,
      onReopen: noop,
      onReply,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
      initialReplyDraft: "a reply to send",
      onReplyDraftChange,
    });
    const ta = getByPlaceholderText("Write a reply...") as HTMLTextAreaElement;
    // Submit via Cmd/Ctrl+Enter; on success CommentForm clears its value, which
    // must propagate out as an empty draft so the surface drops the stored slot.
    await fireEvent.keyDown(ta, { key: "Enter", metaKey: true });
    await vi.waitFor(() => expect(onReply).toHaveBeenCalledWith("thread-9", "a reply to send"));
    await vi.waitFor(() => expect(onReplyDraftChange).toHaveBeenLastCalledWith("thread-9", ""));
  });

  it("keeps reporting the original thread id after the comment prop changes (mount-time capture)", async () => {
    const onReplyDraftChange = vi.fn();
    const { getByPlaceholderText, rerender } = render(CommentThread, {
      comment: mkComment({ id: "first", status: "open" }),
      replies: [],
      active: true,
      onResolve: noop,
      onReopen: noop,
      onReply: asyncNoop,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
      onReplyDraftChange,
    });
    // The surfaces remount per thread, so a live instance never sees comment.id
    // change; this guards the untrack capture that makes that safe regardless.
    await rerender({ comment: mkComment({ id: "second", status: "open" }) });
    const ta = getByPlaceholderText("Write a reply...") as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: "still the first thread" } });
    expect(onReplyDraftChange).toHaveBeenLastCalledWith("first", "still the first thread");
  });
});

describe("CommentThread copy-link button", () => {
  it("renders the copy-link button", () => {
    const { getByRole } = renderThread({ id: "1" });
    expect(getByRole("button", { name: "Copy link" })).toBeTruthy();
  });
});

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
