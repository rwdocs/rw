import { describe, it, expect } from "vitest";
import { formatRelativeTime } from "./formatRelativeTime";

describe("formatRelativeTime", () => {
  const now = new Date("2026-04-24T12:00:00Z");

  it("returns 'just now' for the same instant", () => {
    expect(formatRelativeTime(now, now)).toBe("just now");
  });

  it("returns 'just now' under 60 seconds", () => {
    const date = new Date(now.getTime() - 59 * 1000);
    expect(formatRelativeTime(date, now)).toBe("just now");
  });

  it("returns minutes ago between 1 and 59 minutes", () => {
    expect(formatRelativeTime(new Date(now.getTime() - 60 * 1000), now)).toBe("1m ago");
    expect(formatRelativeTime(new Date(now.getTime() - 59 * 60 * 1000), now)).toBe("59m ago");
  });

  it("returns hours ago between 1 and 23 hours", () => {
    expect(formatRelativeTime(new Date(now.getTime() - 60 * 60 * 1000), now)).toBe("1h ago");
    expect(formatRelativeTime(new Date(now.getTime() - 23 * 60 * 60 * 1000), now)).toBe("23h ago");
  });

  it("returns days ago between 1 and 29 days", () => {
    expect(formatRelativeTime(new Date(now.getTime() - 24 * 60 * 60 * 1000), now)).toBe("1d ago");
    expect(formatRelativeTime(new Date(now.getTime() - 29 * 24 * 60 * 60 * 1000), now)).toBe(
      "29d ago",
    );
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
