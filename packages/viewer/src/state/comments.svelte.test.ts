import { describe, it, expect, vi, beforeEach } from "vitest";
import { Comments } from "./comments.svelte";
import type { Comment } from "../types/comments";
import type { CommentApiClient } from "../api/comments";

const stubClient = {} as CommentApiClient;

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
    canResolve: false,
    ...over,
  };
}

const quoteSel = [{ type: "TextQuoteSelector", exact: "x" }] as Comment["selectors"];

describe("Comments deep-link state", () => {
  it("defaults linkedId to null and resolvedExpanded to false", () => {
    const c = new Comments(stubClient, () => {});
    expect(c.linkedId).toBeNull();
    expect(c.resolvedExpanded).toBe(false);
  });

  // Scoped to the fields this branch adds; clear() pre-dates this branch and its
  // other resets are exercised by the e2e suite.
  it("clear() resets linkedId and resolvedExpanded", () => {
    const c = new Comments(stubClient, () => {});
    c.linkedId = "abc";
    c.resolvedExpanded = true;
    c.clear();
    expect(c.linkedId).toBeNull();
    expect(c.resolvedExpanded).toBe(false);
  });
});

describe("Comments load failures route through notify", () => {
  it("calls notify with an error when the list request rejects", async () => {
    const notify = vi.fn();
    const client = {
      list: vi.fn().mockRejectedValue(new Error("network down")),
    } as unknown as CommentApiClient;
    const c = new Comments(client, notify);
    c.enabled = true;
    await c.load("guide");
    expect(notify).toHaveBeenCalledWith({ intent: "error", message: "network down" });
  });

  it("calls notify with the generic message when the rejection is not an Error", async () => {
    const notify = vi.fn();
    const client = {
      list: vi.fn().mockRejectedValue("boom string"),
    } as unknown as CommentApiClient;
    const c = new Comments(client, notify);
    c.enabled = true;
    await c.load("guide");
    expect(notify).toHaveBeenCalledWith({ intent: "error", message: "Failed to load comments" });
  });
});

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

  it("keeps items and does not notify when a silent load rejects", async () => {
    const notify = vi.fn();
    const items = [mkComment({ id: "c1" })];
    // First call resolves with items; second (silent) call rejects.
    const list = vi
      .fn()
      .mockResolvedValueOnce(items)
      .mockRejectedValueOnce(new Error("network down"));
    const client = { list } as unknown as CommentApiClient;
    const c = new Comments(client, notify);
    c.enabled = true;

    await c.load("a.md");
    expect(c.items).toEqual(items);

    await c.load("a.md", { silent: true });

    // Transient blip is swallowed: items kept, no toast, spinner cleared.
    expect(c.items).toEqual(items);
    expect(notify).not.toHaveBeenCalled();
    expect(c.loading).toBe(false);
  });

  it("clears the spinner when a silent load supersedes an in-flight non-silent load and then fails", async () => {
    const notify = vi.fn();
    const items = [mkComment({ id: "c1" })];
    let resolveFirst: ((v: Comment[]) => void) | undefined;
    const list = vi
      .fn()
      // Seed load: resolves immediately.
      .mockResolvedValueOnce(items)
      // Non-silent navigation: hangs indefinitely.
      .mockImplementationOnce(
        () =>
          new Promise<Comment[]>((res) => {
            resolveFirst = res;
          }),
      )
      // Silent refresh: rejects.
      .mockRejectedValueOnce(new Error("server restarting"));
    const client = { list } as unknown as CommentApiClient;
    const c = new Comments(client, notify);
    c.enabled = true;

    // Seed: populate items.
    await c.load("a.md");
    expect(c.items).toEqual(items);

    // Non-silent load hangs → spinner on.
    const p1 = c.load("a.md");
    expect(c.loading).toBe(true);

    // Silent refresh supersedes it (aborting p1) and then fails.
    await c.load("a.md", { silent: true });

    // Spinner cleared, items preserved, no toast.
    expect(c.loading).toBe(false);
    expect(c.items).toEqual(items);
    expect(notify).not.toHaveBeenCalled();

    // Let the dangling promise settle cleanly.
    resolveFirst?.([]);
    await p1.catch(() => {});
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
    comments = new Comments(stubClient, () => {});
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

describe("Comments.subscribe facade", () => {
  it("delegates to the client and returns its unsubscribe handle", () => {
    const unsub = vi.fn();
    const subscribe = vi.fn().mockReturnValue(unsub);
    const client = {
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
      subscribe,
    } as unknown as CommentApiClient;
    const comments = new Comments(client, vi.fn());

    expect(comments.canSubscribe).toBe(true);
    const onChange = () => {};
    const ret = comments.subscribe("doc-1", onChange);
    expect(subscribe).toHaveBeenCalledWith("doc-1", onChange);
    expect(ret).toBe(unsub);
  });

  it("reports canSubscribe=false and returns undefined when the client has no subscribe", () => {
    const client = {
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    } as unknown as CommentApiClient;
    const comments = new Comments(client, vi.fn());

    expect(comments.canSubscribe).toBe(false);
    expect(comments.subscribe("doc-1", () => {})).toBeUndefined();
  });
});
