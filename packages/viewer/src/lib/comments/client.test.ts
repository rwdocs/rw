import { describe, it, expect, vi } from "vitest";
import { selectCommentClient } from "./client";
import type { CommentApiClient } from "../../api/comments";

describe("selectCommentClient", () => {
  it("uses the injected client and marks comments enabled", () => {
    const injected = {
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    } as unknown as CommentApiClient;

    const { client, enabled } = selectCommentClient(injected, "/_api");
    expect(client).toBe(injected);
    expect(enabled).toBe(true);
  });

  it("builds the default HTTP client bound to apiBaseUrl when none injected", async () => {
    // Behavioural check: the returned client must be the real HTTP client wired
    // to the given base + fetchFn (not merely an object of the right shape), and
    // `enabled` must be false so the /config flag still governs the default path.
    const fetchFn = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [],
    });

    const { client, enabled } = selectCommentClient(
      undefined,
      "/_api",
      fetchFn as unknown as typeof fetch,
    );
    expect(enabled).toBe(false);

    await client.list("page-1");
    expect(fetchFn).toHaveBeenCalledWith(
      expect.stringContaining("/_api/comments?documentId=page-1"),
      expect.anything(),
    );
  });
});
