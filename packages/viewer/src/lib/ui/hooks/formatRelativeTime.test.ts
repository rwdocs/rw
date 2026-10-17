// @vitest-environment node

import { describe, it, expect } from "vitest";
import { formatRelativeTime } from "./formatRelativeTime";

describe("formatRelativeTime", () => {
  const now = new Date("2026-04-24T12:00:00Z");

  const SEC = 1000;
  const MIN = 60 * SEC;
  const HOUR = 60 * MIN;
  const DAY = 24 * HOUR;

  it.each([
    [0, "just now"],
    [59 * SEC, "just now"],
    [MIN, "1m ago"],
    [59 * MIN, "59m ago"],
    [HOUR, "1h ago"],
    [23 * HOUR, "23h ago"],
    [DAY, "1d ago"],
    [29 * DAY, "29d ago"],
  ])("formats an age of %ims as %j", (age, expected) => {
    expect(formatRelativeTime(new Date(now.getTime() - age), now)).toBe(expected);
  });

  it("falls back to localized date for 30+ days", () => {
    const date = new Date(now.getTime() - 30 * 24 * 60 * 60 * 1000);
    expect(formatRelativeTime(date, now)).toBe(date.toLocaleDateString());
  });

  it("uses real-time `now` when omitted", () => {
    const result = formatRelativeTime(new Date());
    expect(result).toBe("just now");
  });
});
