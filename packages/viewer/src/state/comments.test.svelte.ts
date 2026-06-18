import { describe, it, expect, vi } from "vitest";
import { Comments } from "./comments.svelte";
import type { CommentApiClient } from "../api/comments";

function makeClient(items: unknown[] = []): CommentApiClient {
  return {
    list: vi.fn(async () => items),
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
  } as unknown as CommentApiClient;
}

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
    const comments = new Comments(client);
    comments.enabled = true;
    const p1 = comments.load("a.md"); // non-silent → loading=true, stays pending
    expect(comments.loading).toBe(true);
    await comments.load("a.md", { silent: true }); // aborts p1, wins, must clear loading
    expect(comments.loading).toBe(false);
    resolveFirst?.([]); // let the dangling first promise settle
    await p1.catch(() => {});
  });

  it("does not toggle loading when silent", async () => {
    const comments = new Comments(makeClient());
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
    const comments = new Comments(makeClient());
    comments.enabled = true;
    const p = comments.load("a.md");
    expect(comments.loading).toBe(true);
    await p;
    expect(comments.loading).toBe(false);
  });

  it("preserves a pending draft on a silent refetch of the same document", async () => {
    const comments = new Comments(makeClient());
    comments.enabled = true;
    await comments.load("a.md");
    comments.pending = { documentId: "a.md", selectors: [] };
    await comments.load("a.md", { silent: true });
    expect(comments.pending).not.toBeNull();
  });
});
