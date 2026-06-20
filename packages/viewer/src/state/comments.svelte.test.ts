import { describe, it, expect, vi } from "vitest";
import { Comments } from "./comments.svelte";
import type { CommentApiClient } from "../api/comments";

const stubClient = {} as CommentApiClient;

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
