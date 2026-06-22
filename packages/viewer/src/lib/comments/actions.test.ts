import { describe, it, expect, vi } from "vitest";
import { createCommentActions } from "./actions";
import { SAVE_FAILED_MESSAGE } from "./messages";

type CommentsStub = Parameters<typeof createCommentActions>[0];

function makeComments(over: Partial<Record<string, unknown>> = {}): CommentsStub {
  return {
    resolve: vi.fn(async () => {}),
    reopen: vi.fn(async () => {}),
    delete: vi.fn(async () => {}),
    restore: vi.fn(async () => {}),
    create: vi.fn(async () => ({})),
    threads: [],
    ...over,
  } as unknown as CommentsStub;
}

describe("createCommentActions", () => {
  it("resolve delegates to comments.resolve and swallows errors with a notify", async () => {
    const notify = vi.fn();
    const comments = makeComments({
      resolve: vi.fn(async () => {
        throw new Error("boom");
      }),
    });
    const actions = createCommentActions(comments, notify);

    await actions.resolve("c1"); // must NOT throw
    expect(comments.resolve).toHaveBeenCalledWith("c1");
    expect(notify).toHaveBeenCalledWith({ intent: "error", message: "boom" });
  });

  it("resolve uses the generic fallback when the rejection is not an Error", async () => {
    const notify = vi.fn();
    const comments = makeComments({
      resolve: vi.fn(async () => {
        throw "nope";
      }),
    });
    await createCommentActions(comments, notify).resolve("c1");
    expect(notify).toHaveBeenCalledWith({ intent: "error", message: "Failed to resolve comment" });
  });

  it("reopen / remove / restore delegate to the matching store method", async () => {
    const notify = vi.fn();
    const comments = makeComments();
    const actions = createCommentActions(comments, notify);

    await actions.reopen("c1");
    await actions.remove("c2");
    await actions.restore("c3");

    expect(comments.reopen).toHaveBeenCalledWith("c1");
    expect(comments.delete).toHaveBeenCalledWith("c2");
    expect(comments.restore).toHaveBeenCalledWith("c3");
    expect(notify).not.toHaveBeenCalled();
  });

  it("reopen / remove / restore notify with their own fallback string on a non-Error failure", async () => {
    // Throw a non-Error so the per-action fallback string is used (an Error
    // would surface its own `.message` instead — see the resolve cases above).
    const cases: Array<["reopen" | "remove" | "restore", string, string]> = [
      ["reopen", "reopen", "Failed to reopen comment"],
      ["remove", "delete", "Failed to delete comment"],
      ["restore", "restore", "Failed to restore comment"],
    ];
    for (const [action, method, message] of cases) {
      const notify = vi.fn();
      const comments = makeComments({
        [method]: vi.fn(async () => {
          throw "boom";
        }),
      });
      const actions = createCommentActions(comments, notify);
      await actions[action]("id");
      expect(notify).toHaveBeenCalledWith({ intent: "error", message });
    }
  });

  it("reply looks up the parent thread, creates a reply, and resolves on success", async () => {
    const notify = vi.fn();
    const create = vi.fn(async () => ({}) as never);
    const comments = makeComments({
      create,
      threads: [{ id: "p1", documentId: "a.md" }],
    });
    const actions = createCommentActions(comments, notify);

    await actions.reply("p1", "hello");

    expect(create).toHaveBeenCalledWith({
      documentId: "a.md",
      parentId: "p1",
      body: "hello",
      selectors: [],
    });
    expect(notify).not.toHaveBeenCalled();
  });

  it("reply returns early (no create, no notify) when the parent thread is missing", async () => {
    const notify = vi.fn();
    const create = vi.fn();
    const comments = makeComments({ create, threads: [] });
    await createCommentActions(comments, notify).reply("ghost", "hi");
    expect(create).not.toHaveBeenCalled();
    expect(notify).not.toHaveBeenCalled();
  });

  it("reply notifies with SAVE_FAILED_MESSAGE and rethrows when create fails", async () => {
    const notify = vi.fn();
    const err = new Error("save failed");
    const comments = makeComments({
      create: vi.fn(async () => {
        throw err;
      }),
      threads: [{ id: "p1", documentId: "a.md" }],
    });
    const actions = createCommentActions(comments, notify);

    await expect(actions.reply("p1", "hello")).rejects.toBe(err);
    expect(notify).toHaveBeenCalledWith({ intent: "error", message: SAVE_FAILED_MESSAGE });
  });
});
