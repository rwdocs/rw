import { describe, it, expect } from "vitest";
import { Comments } from "./comments.svelte";
import type { CommentApiClient } from "../api/comments";

const stubClient = {} as CommentApiClient;

describe("Comments deep-link state", () => {
  it("defaults linkedId to null and resolvedExpanded to false", () => {
    const c = new Comments(stubClient);
    expect(c.linkedId).toBeNull();
    expect(c.resolvedExpanded).toBe(false);
  });

  // Scoped to the fields this branch adds; clear() pre-dates this branch and its
  // other resets are exercised by the e2e suite.
  it("clear() resets linkedId and resolvedExpanded", () => {
    const c = new Comments(stubClient);
    c.linkedId = "abc";
    c.resolvedExpanded = true;
    c.clear();
    expect(c.linkedId).toBeNull();
    expect(c.resolvedExpanded).toBe(false);
  });
});
