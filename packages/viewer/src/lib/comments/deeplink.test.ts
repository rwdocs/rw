import { describe, it, expect } from "vitest";
import {
  buildCommentHash,
  parseCommentHash,
  isCommentHash,
  classifyCommentTarget,
} from "./deeplink";
import type { Comment } from "../../types/comments";

function makeComment(overrides: Partial<Comment> = {}): Comment {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    documentId: "",
    author: { id: "local:human", name: "You" },
    body: "hi",
    selectors: [],
    status: "open",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    canDelete: false,
    canRestore: false,
    ...overrides,
  };
}

describe("buildCommentHash", () => {
  it("prefixes the id", () => {
    expect(buildCommentHash("abc")).toBe("comment-abc");
  });
});

describe("parseCommentHash", () => {
  it("extracts the id with a leading #", () => {
    expect(parseCommentHash("#comment-abc")).toBe("abc");
  });
  it("extracts the id without a leading #", () => {
    expect(parseCommentHash("comment-abc")).toBe("abc");
  });
  it("returns null for a non-comment hash", () => {
    expect(parseCommentHash("#my-heading")).toBeNull();
  });
  it("parses a comment-prefixed hash even when the id looks like a heading slug", () => {
    // The membership check in isCommentHash (not the prefix) decides whether this
    // is a real comment; parseCommentHash only strips the prefix.
    expect(parseCommentHash("comment-guidelines-heading")).toBe("guidelines-heading");
  });
  it("returns null for an empty id or empty hash", () => {
    expect(parseCommentHash("comment-")).toBeNull();
    expect(parseCommentHash("")).toBeNull();
  });
});

describe("isCommentHash", () => {
  it("is true only when the id is a known comment", () => {
    expect(isCommentHash("#comment-abc", ["abc", "def"])).toBe(true);
    expect(isCommentHash("#comment-xyz", ["abc", "def"])).toBe(false);
    expect(isCommentHash("#my-heading", ["abc"])).toBe(false);
  });
  it("returns false for an empty id set (e.g. before comments load)", () => {
    // The heading-scroll effect relies on this: with no comments loaded yet, a
    // `#comment-…` hash must NOT be claimed as a comment, so headings still scroll.
    expect(isCommentHash("#comment-abc", [])).toBe(false);
  });
});

describe("classifyCommentTarget", () => {
  it("missing when the comment is absent", () => {
    expect(classifyCommentTarget(undefined, false)).toBe("missing");
  });
  it("resolved regardless of anchoring", () => {
    expect(classifyCommentTarget(makeComment({ status: "resolved" }), true)).toBe("resolved");
  });
  it("inline when open and anchored", () => {
    expect(classifyCommentTarget(makeComment(), true)).toBe("inline");
  });
  it("page when open but not anchored", () => {
    expect(classifyCommentTarget(makeComment(), false)).toBe("page");
  });
});
