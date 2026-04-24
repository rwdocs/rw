import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import LoadingSkeleton from "./LoadingSkeleton.svelte";

describe("LoadingSkeleton", () => {
  it("exposes a loading status with an accessible label", () => {
    const { getByRole } = render(LoadingSkeleton);
    const status = getByRole("status", { name: "Loading content" });
    expect(status).toBeTruthy();
  });

  it("includes a screen-reader-only text fallback", () => {
    const { getByText } = render(LoadingSkeleton);
    expect(getByText("Loading...")).toBeTruthy();
  });

  it("uses the bg-placeholder semantic token (no palette leaks)", () => {
    const { getByRole } = render(LoadingSkeleton);
    const html = getByRole("status").innerHTML;
    expect(html).toContain("bg-bg-placeholder");
    expect(html).not.toMatch(/bg-(gray|neutral|slate|zinc|stone)-\d/);
  });
});
