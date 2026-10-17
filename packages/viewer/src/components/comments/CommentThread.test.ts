import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

import CommentThread from "./CommentThread.svelte";
import type { Comment } from "../../types/comments";

// Re-stub ResizeObserver per test so the afterEach teardown (which restores any
// globals a test stubbed, e.g. navigator in the copy-link test) cannot leave a
// later test without it.
beforeEach(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});

afterEach(() => {
  vi.unstubAllGlobals();
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

describe("CommentThread copy-link announcement", () => {
  it("announces the copy in a polite live region, then clears it after the reset", async () => {
    // Fake timers make the 1500ms auto-reset deterministic and let us assert the
    // region returns to empty (so it won't keep announcing on the next focus).
    vi.useFakeTimers();
    try {
      // copyLink() only touches navigator.clipboard.writeText, so stub just that
      // surface. (A {...navigator} spread would drop navigator's prototype getters
      // and yield a degraded object.) The afterEach restores it.
      vi.stubGlobal("navigator", {
        clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
      });
      const { getByRole, container } = renderThread({ id: "1" });

      const status = container.querySelector<HTMLElement>('[aria-live="polite"]')!;
      expect(status).not.toBeNull();
      expect(status.getAttribute("aria-atomic")).toBe("true");
      expect(status.textContent?.trim()).toBe("");

      await fireEvent.click(getByRole("button", { name: "Copy link" }));
      // copyLink awaits the (immediately-resolved) clipboard write before setting
      // copied=true; advancing fake timers flushes that microtask, then tick()
      // flushes Svelte's DOM update.
      await vi.advanceTimersByTimeAsync(0);
      await tick();
      expect(status.textContent?.trim()).toBe("Link copied");

      await vi.advanceTimersByTimeAsync(1500);
      await tick();
      expect(status.textContent?.trim()).toBe("");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("CommentThread canResolve gating", () => {
  it.each([
    { canResolve: true, status: "open", button: "Resolve", shown: true },
    { canResolve: false, status: "open", button: "Resolve", shown: false },
    { canResolve: true, status: "resolved", button: "Reopen", shown: true },
    { canResolve: false, status: "resolved", button: "Reopen", shown: false },
  ] as const)(
    "$button is shown=$shown when canResolve=$canResolve on a $status comment",
    ({ canResolve, status, button, shown }) => {
      const { queryByRole } = renderThread({ id: "1", canResolve, status });
      expect(queryByRole("button", { name: button }) !== null).toBe(shown);
    },
  );

  it("swaps Resolve for Reopen when the comment prop resolves", async () => {
    const onReopen = vi.fn();
    const props = {
      comment: mkComment({ id: "1" }),
      replies: [],
      onResolve: noop,
      onReopen,
      onReply: asyncNoop,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
    };
    const { getByRole, rerender } = render(CommentThread, props);

    await rerender({ ...props, comment: mkComment({ id: "1", status: "resolved" }) });
    await fireEvent.click(getByRole("button", { name: "Reopen" }));

    expect(onReopen).toHaveBeenCalledWith("1");
  });
});

const FUZZY_LABEL = "Re-anchored to the closest matching passage";

function renderFuzzyThread(opts: {
  fuzzy?: boolean;
  nav?: { index: number; total: number; onPrev: () => void; onNext: () => void };
}) {
  return render(CommentThread, {
    comment: mkComment({ id: "1" }),
    replies: [],
    onResolve: noop,
    onReopen: noop,
    onReply: asyncNoop,
    onDelete: asyncNoop,
    onRestore: asyncNoop,
    onClose: noop,
    ...opts,
  });
}

describe("CommentThread fuzzy badge", () => {
  it("shows 'fuzzy' but announces the re-anchor label on a multi-comment page", () => {
    const { getByText } = renderFuzzyThread({
      fuzzy: true,
      nav: { index: 2, total: 10, onPrev: noop, onNext: noop },
    });
    expect(getByText(FUZZY_LABEL)).toBeTruthy();
    expect(getByText("fuzzy").getAttribute("aria-hidden")).toBe("true");
  });

  it("keeps the badge out of the avatar row, so the author name gets the full width", () => {
    const { getByText, container } = renderFuzzyThread({
      fuzzy: true,
      nav: { index: 2, total: 10, onPrev: noop, onNext: noop },
    });
    const avatarRow = container.querySelector('[data-testid="comment-avatar-row"]')!;
    expect(avatarRow.contains(getByText(FUZZY_LABEL))).toBe(false);
  });

  it("still renders the badge on a single-comment page, where the counter is suppressed", () => {
    // total === 1 is what suppresses the counter in production (CommentPanel
    // always passes nav, even on a single-comment page) => close-only header branch.
    const { getByText, queryByText } = renderFuzzyThread({
      fuzzy: true,
      nav: { index: 0, total: 1, onPrev: noop, onNext: noop },
    });
    expect(getByText(FUZZY_LABEL)).toBeTruthy();
    expect(queryByText("1 / 1")).toBeNull();
  });

  it("renders no badge when the comment is exactly anchored", () => {
    const { queryByText } = renderFuzzyThread({
      nav: { index: 2, total: 10, onPrev: noop, onNext: noop },
    });
    expect(queryByText(FUZZY_LABEL)).toBeNull();
  });

  it("does not render the badge on a surface with no onClose (page-comments list)", () => {
    // The header block — and with it the badge — sits inside `{#if onClose}`,
    // so a surface that omits onClose shows no badge even when fuzzy is set.
    const { queryByText } = render(CommentThread, {
      comment: mkComment({ id: "1" }),
      replies: [],
      onResolve: noop,
      onReopen: noop,
      onReply: asyncNoop,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
      fuzzy: true,
      // onClose intentionally omitted
    });
    expect(queryByText(FUZZY_LABEL)).toBeNull();
  });
});

describe("CommentThread focus after reply", () => {
  function threadProps(onReply: (parentId: string, body: string) => Promise<void>) {
    return {
      comment: mkComment({ id: "7", status: "open" }),
      replies: [],
      onResolve: noop,
      onReopen: noop,
      onReply,
      onDelete: asyncNoop,
      onRestore: asyncNoop,
    };
  }

  async function submitReply(ta: HTMLTextAreaElement) {
    ta.focus();
    await fireEvent.input(ta, { target: { value: "a reply" } });
    await fireEvent.keyDown(ta, { key: "Enter", ctrlKey: true });
  }

  it("focuses the thread card itself when it has a domId (page-comments list)", async () => {
    const { getByPlaceholderText, getByTestId } = render(CommentThread, {
      ...threadProps(asyncNoop),
      domId: "comment-7",
    });
    await submitReply(getByPlaceholderText("Write a reply...") as HTMLTextAreaElement);
    await vi.waitFor(() => expect(document.activeElement).toBe(getByTestId("comment-thread")));
  });

  it("focuses the nearest tabindex=-1 wrapper when it has no domId (sidebar card)", async () => {
    const wrapper = document.createElement("div");
    wrapper.tabIndex = -1;
    document.body.appendChild(wrapper);
    try {
      const { getByPlaceholderText } = render(CommentThread, {
        target: wrapper,
        props: threadProps(asyncNoop),
      });
      await submitReply(getByPlaceholderText("Write a reply...") as HTMLTextAreaElement);
      await vi.waitFor(() => expect(document.activeElement).toBe(wrapper));
    } finally {
      wrapper.remove();
    }
  });

  it("keeps focus and the draft in the textarea when the reply fails", async () => {
    const onReply = vi.fn().mockRejectedValue(new Error("save failed"));
    const { getByPlaceholderText, findByRole } = render(CommentThread, {
      ...threadProps(onReply),
      domId: "comment-7",
    });
    const ta = getByPlaceholderText("Write a reply...") as HTMLTextAreaElement;
    await submitReply(ta);
    await findByRole("button", { name: "Retry" });
    expect(onReply).toHaveBeenCalledWith("7", "a reply");
    expect(document.activeElement).toBe(ta);
    expect(ta.value).toBe("a reply");
  });
});
