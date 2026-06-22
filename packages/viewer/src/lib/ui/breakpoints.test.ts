import { describe, it, expect } from "vitest";
import { COMMENTS_BREAKPOINT_PX, isLayoutNarrow } from "./breakpoints";

describe("isLayoutNarrow", () => {
  it("is false before measurement (width 0) to default to the desktop aside", () => {
    expect(isLayoutNarrow(0)).toBe(false);
  });

  it("is true below the comments breakpoint", () => {
    expect(isLayoutNarrow(COMMENTS_BREAKPOINT_PX - 1)).toBe(true);
    expect(isLayoutNarrow(700)).toBe(true);
  });

  it("is false at or above the comments breakpoint", () => {
    expect(isLayoutNarrow(COMMENTS_BREAKPOINT_PX)).toBe(false);
    expect(isLayoutNarrow(1400)).toBe(false);
  });
});
