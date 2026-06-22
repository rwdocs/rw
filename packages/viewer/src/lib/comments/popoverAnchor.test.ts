import { describe, it, expect } from "vitest";
import { COMMENT_POPOVER_WIDTH_PX, clampPopoverLeft } from "./popoverAnchor";

describe("clampPopoverLeft", () => {
  const W = COMMENT_POPOVER_WIDTH_PX;

  it("centers on the highlight when there is room on both sides", () => {
    // container 800 wide, highlight centered at 400 → left = 400 - W/2.
    expect(clampPopoverLeft(400, 800)).toBe(400 - W / 2);
  });

  it("pins to the left margin when the highlight is near the start", () => {
    expect(clampPopoverLeft(10, 800)).toBe(8);
  });

  it("pins to the right edge when the highlight is near the end", () => {
    // max left = containerWidth - W - margin.
    expect(clampPopoverLeft(790, 800)).toBe(800 - W - 8);
  });

  it("pins to the margin when the container is narrower than the popover", () => {
    expect(clampPopoverLeft(150, 300)).toBe(8);
  });
});
