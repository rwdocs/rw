import { describe, it, expect, vi, beforeEach } from "vitest";
import { Comments } from "./comments.svelte";
import type { Comment } from "../types/comments";
import type { CommentApiClient } from "../api/comments";

function makeClient(items: unknown[] = []): CommentApiClient {
  return {
    list: vi.fn(async () => items),
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
  } as unknown as CommentApiClient;
}

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
    ...over,
  };
}

const quoteSel = [{ type: "TextQuoteSelector", exact: "x" }] as Comment["selectors"];

describe("Comments.load", () => {
  it("clears loading when a silent load supersedes an in-flight non-silent load", async () => {
    let resolveFirst: ((v: unknown[]) => void) | undefined;
    const client = {
      list: vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise((res) => {
              resolveFirst = res as typeof resolveFirst;
            }),
        )
        .mockImplementation(async () => []),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    } as unknown as CommentApiClient;
    const comments = new Comments(client, () => {});
    comments.enabled = true;
    const p1 = comments.load("a.md"); // non-silent → loading=true, stays pending
    expect(comments.loading).toBe(true);
    await comments.load("a.md", { silent: true }); // aborts p1, wins, must clear loading
    expect(comments.loading).toBe(false);
    resolveFirst?.([]); // let the dangling first promise settle
    await p1.catch(() => {});
  });

  it("does not toggle loading when silent", async () => {
    const comments = new Comments(makeClient(), () => {});
    comments.enabled = true;
    const states: boolean[] = [];
    // Sample loading right after kicking off a silent load.
    const p = comments.load("a.md", { silent: true });
    states.push(comments.loading);
    await p;
    expect(states).toEqual([false]);
    expect(comments.loading).toBe(false);
  });

  it("sets loading during a non-silent load", async () => {
    const comments = new Comments(makeClient(), () => {});
    comments.enabled = true;
    const p = comments.load("a.md");
    expect(comments.loading).toBe(true);
    await p;
    expect(comments.loading).toBe(false);
  });

  it("preserves a pending draft on a silent refetch of the same document", async () => {
    const comments = new Comments(makeClient(), () => {});
    comments.enabled = true;
    await comments.load("a.md");
    comments.pending = { documentId: "a.md", selectors: [] };
    await comments.load("a.md", { silent: true });
    expect(comments.pending).not.toBeNull();
  });
});

describe("Comments navigation", () => {
  let comments: Comments;

  beforeEach(() => {
    comments = new Comments({} as CommentApiClient, () => {});
  });

  it("orders navigable: inline (by order rank) then page comments (by createdAt)", () => {
    comments.items = [
      mkComment({ id: "p2", createdAt: "2026-01-02T00:00:00Z" }), // page comment
      mkComment({ id: "i2", selectors: quoteSel }), // inline
      mkComment({ id: "p1", createdAt: "2026-01-01T00:00:00Z" }), // page comment
      mkComment({ id: "i1", selectors: quoteSel }), // inline
    ];
    comments.order = ["i1", "i2"]; // DOM order of inline threads
    expect(comments.navigable).toEqual(["i1", "i2", "p1", "p2"]);
  });

  it("excludes resolved threads from navigable", () => {
    comments.items = [
      mkComment({ id: "i1", selectors: quoteSel }),
      mkComment({ id: "i2", selectors: quoteSel, status: "resolved" }),
      mkComment({ id: "p1" }),
    ];
    comments.order = ["i1", "i2"];
    expect(comments.navigable).toEqual(["i1", "p1"]);
  });

  it("navigate enters at first on next and last on prev from idle", () => {
    comments.items = [mkComment({ id: "i1", selectors: quoteSel }), mkComment({ id: "p1" })];
    comments.order = ["i1"];

    const first = comments.navigate("next");
    expect(comments.activeId).toBe("i1");
    expect(first).toEqual({ index: 0, total: 2, author: "You" });

    comments.activeId = null;
    const last = comments.navigate("prev");
    expect(comments.activeId).toBe("p1");
    expect(last).toEqual({ index: 1, total: 2, author: "You" });
  });

  it("navigate wraps and bumps navSeq each call", () => {
    comments.items = [
      mkComment({ id: "i1", selectors: quoteSel }),
      mkComment({ id: "i2", selectors: quoteSel }),
    ];
    comments.order = ["i1", "i2"];
    const before = comments.navSeq;

    comments.activeId = "i2";
    comments.navigate("next"); // wraps to i1
    expect(comments.activeId).toBe("i1");
    comments.navigate("prev"); // wraps to i2
    expect(comments.activeId).toBe("i2");
    expect(comments.navSeq).toBe(before + 2);
  });

  it("navigate returns null and does nothing with no navigable comments", () => {
    comments.items = [mkComment({ id: "i1", selectors: quoteSel, status: "resolved" })];
    expect(comments.navigate("next")).toBeNull();
    expect(comments.activeId).toBeNull();
  });

  it("activeIsInline is true for an active inline thread (page comments excluded)", () => {
    comments.items = [mkComment({ id: "i1", selectors: quoteSel }), mkComment({ id: "p1" })];
    comments.order = ["i1"];
    expect(comments.activeIsInline).toBe(false);
    comments.activeId = "i1";
    expect(comments.activeIsInline).toBe(true);
    comments.activeId = "p1";
    expect(comments.activeIsInline).toBe(false);
  });

  it("activeIsInline stays true for a resolved active inline thread (sidebar still shows it)", () => {
    comments.items = [mkComment({ id: "i1", selectors: quoteSel, status: "resolved" })];
    comments.activeId = "i1";
    // A resolved inline thread is excluded from `navigable` but can still be the
    // active thread (e.g. clicked from the resolved list), and the sidebar must
    // render it — so activeIsInline does not filter by status.
    expect(comments.activeIsInline).toBe(true);
  });
});
