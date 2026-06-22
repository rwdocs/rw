import { describe, it, expect, vi, beforeAll } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import CommentForm from "./CommentForm.svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

beforeAll(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});

const PLACEHOLDER = "Write a comment...";

async function fillDraft(textarea: HTMLElement, value: string) {
  await fireEvent.input(textarea, { target: { value } });
}

describe("CommentForm bindable value", () => {
  it("renders the textarea seeded from the value prop", () => {
    const { getByPlaceholderText } = render(CommentForm, {
      onSubmit: vi.fn().mockResolvedValue(undefined),
      value: "restored draft",
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    expect(ta.value).toBe("restored draft");
  });
});

describe("CommentForm keep-on-failure", () => {
  it("keeps the text and shows Retry when onSubmit rejects", async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error("server down"));
    const { getByPlaceholderText, getByRole } = render(CommentForm, {
      onSubmit,
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    await fillDraft(ta, "a draft I do not want to lose");
    await fireEvent.keyDown(ta, { key: "Enter", metaKey: true });
    await vi.waitFor(() => getByRole("button", { name: "Retry" }));
    expect(ta.value).toBe("a draft I do not want to lose");
  });

  it("clears the text and reverts to Comment on success", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const { getByPlaceholderText, getByRole } = render(CommentForm, {
      onSubmit,
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    await fillDraft(ta, "hello");
    await fireEvent.keyDown(ta, { key: "Enter", metaKey: true });
    await vi.waitFor(() => expect(ta.value).toBe(""));
    expect(getByRole("button", { name: "Comment" }).textContent?.trim()).toBe("Comment");
  });

  it("clears the failed state when the user edits again", async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error("nope"));
    const { getByPlaceholderText, getByRole } = render(CommentForm, {
      onSubmit,
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    await fillDraft(ta, "draft");
    await fireEvent.keyDown(ta, { key: "Enter", metaKey: true });
    await vi.waitFor(() => getByRole("button", { name: "Retry" }));
    await fillDraft(ta, "draft more");
    expect(getByRole("button", { name: "Comment" }).textContent?.trim()).toBe("Comment");
  });

  it("keeps the text and shows Retry when onSubmit rejects via Ctrl+Enter (Windows/Linux)", async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error("server down"));
    const { getByPlaceholderText, getByRole } = render(CommentForm, {
      onSubmit,
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    await fillDraft(ta, "a draft I do not want to lose");
    await fireEvent.keyDown(ta, { key: "Enter", ctrlKey: true });
    await vi.waitFor(() => getByRole("button", { name: "Retry" }));
    expect(ta.value).toBe("a draft I do not want to lose");
  });
});

describe("CommentForm Escape releases focus", () => {
  it("blurs the textarea and calls onCancel when provided", async () => {
    const onCancel = vi.fn();
    const { getByPlaceholderText } = render(CommentForm, {
      onSubmit: vi.fn().mockResolvedValue(undefined),
      onCancel,
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    ta.focus();
    expect(document.activeElement).toBe(ta);

    await fireEvent.keyDown(ta, { key: "Escape" });

    expect(document.activeElement).not.toBe(ta);
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("blurs the textarea on Escape even with no onCancel", async () => {
    const { getByPlaceholderText } = render(CommentForm, {
      onSubmit: vi.fn().mockResolvedValue(undefined),
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    ta.focus();
    expect(document.activeElement).toBe(ta);

    await fireEvent.keyDown(ta, { key: "Escape" });

    expect(document.activeElement).not.toBe(ta);
  });

  it("does not blur or cancel on Escape during IME composition", async () => {
    const onCancel = vi.fn();
    const { getByPlaceholderText } = render(CommentForm, {
      onSubmit: vi.fn().mockResolvedValue(undefined),
      onCancel,
      pinActions: true,
    });
    const ta = getByPlaceholderText(PLACEHOLDER) as HTMLTextAreaElement;
    ta.focus();
    expect(document.activeElement).toBe(ta);

    // Escape mid-composition cancels the IME composition, not the field.
    await fireEvent.keyDown(ta, { key: "Escape", isComposing: true });

    expect(document.activeElement).toBe(ta);
    expect(onCancel).not.toHaveBeenCalled();
  });
});
