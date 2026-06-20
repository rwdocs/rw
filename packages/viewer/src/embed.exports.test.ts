import { describe, it, expect } from "vitest";
import { mountRw } from "./embed";
import type {
  CommentApiClient,
  Comment,
  CreateCommentRequest,
  UpdateCommentRequest,
  Author,
  Selector,
  CommentStatus,
} from "./embed";

// Compile-time guard for the public type surface. Types erase at runtime, so
// there is nothing to assert in a Vitest case; instead, referencing every
// re-exported type in this tuple makes `svelte-check` (run in CI) fail if any
// of them is dropped from the package entry. Exported so it is not flagged as
// an unused declaration.
export type _PublicTypeSurface = [
  CommentApiClient,
  Comment,
  CreateCommentRequest,
  UpdateCommentRequest,
  Author,
  Selector,
  CommentStatus,
];

describe("public exports", () => {
  it("exports mountRw as a function", () => {
    expect(typeof mountRw).toBe("function");
  });
});
