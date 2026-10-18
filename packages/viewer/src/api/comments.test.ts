import { describe, it, expect, vi } from "vitest";
import { createCommentApiClient } from "./comments";
import type { CommentApiClient } from "./comments";

const signal = new AbortController().signal;

const json = { "Content-Type": "application/json" };

const cases: {
  name: string;
  call: (client: CommentApiClient) => Promise<unknown>;
  url: string;
  init: RequestInit;
  failure: string;
}[] = [
  {
    name: "list",
    call: (c) => c.list("doc/1", { signal }),
    url: "/api/rw/comments?documentId=doc%2F1",
    init: { signal },
    failure: "Failed to fetch comments",
  },
  {
    name: "create",
    call: (c) => c.create({ documentId: "doc", body: "hi", selectors: [] }),
    url: "/api/rw/comments",
    init: {
      method: "POST",
      headers: json,
      body: '{"documentId":"doc","body":"hi","selectors":[]}',
    },
    failure: "Failed to create comment",
  },
  {
    name: "update",
    call: (c) => c.update("c1", { status: "resolved" }),
    url: "/api/rw/comments/c1",
    init: { method: "PATCH", headers: json, body: '{"status":"resolved"}' },
    failure: "Failed to update comment",
  },
  {
    name: "delete",
    call: (c) => c.delete("c1"),
    url: "/api/rw/comments/c1",
    init: { method: "DELETE" },
    failure: "Failed to delete comment",
  },
];

describe("createCommentApiClient", () => {
  it.each(cases)("$name requests $url", async ({ call, url, init }) => {
    const body = { id: "c1" };
    const fetchFn = vi.fn(() => Promise.resolve(Response.json(body)));

    await expect(call(createCommentApiClient("/api/rw/", fetchFn))).resolves.toEqual(body);
    expect(fetchFn).toHaveBeenCalledWith(url, init);
  });

  it.each(cases)("$name throws with the status on a non-ok response", async ({ call, failure }) => {
    const fetchFn = vi.fn(() =>
      Promise.resolve(new Response(null, { status: 500, statusText: "Internal Server Error" })),
    );

    await expect(call(createCommentApiClient("/api/rw", fetchFn))).rejects.toThrow(
      `${failure}: 500 Internal Server Error`,
    );
  });
});
