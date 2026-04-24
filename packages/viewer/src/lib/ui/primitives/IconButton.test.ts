import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/IconButtonHarness.svelte";

describe("IconButton", () => {
  it("renders a button with the given aria-label", () => {
    const { getByRole } = render(Harness, { "aria-label": "Open menu" });
    expect(getByRole("button", { name: "Open menu" })).toBeTruthy();
  });

  it("renders children", () => {
    const { getByTestId } = render(Harness, { "aria-label": "X" });
    expect(getByTestId("icon")).toBeTruthy();
  });

  it("uses semantic tokens for colors (no palette leaks)", () => {
    const { getByRole } = render(Harness, { "aria-label": "X" });
    const cls = getByRole("button").className;
    expect(cls).toContain("bg-bg-raised");
    expect(cls).toContain("border-border-default");
    expect(cls).toContain("text-fg-muted");
  });

  it("applies pressed-style classes when active", () => {
    const { getByRole } = render(Harness, { "aria-label": "X", active: true });
    const cls = getByRole("button").className;
    expect(cls).toContain("border-border-strong");
  });

  it("fires onclick when clicked", async () => {
    const onclick = vi.fn();
    const { getByRole } = render(Harness, { "aria-label": "X", onclick });
    await fireEvent.click(getByRole("button"));
    expect(onclick).toHaveBeenCalledTimes(1);
  });

  it("forwards extra attributes (e.g. Popover controlProps)", () => {
    const { getByRole } = render(Harness, {
      "aria-label": "X",
      "aria-controls": "panel-1",
      "aria-expanded": true,
    });
    const button = getByRole("button");
    expect(button.getAttribute("aria-controls")).toBe("panel-1");
    expect(button.getAttribute("aria-expanded")).toBe("true");
  });

  it("merges extra class prop", () => {
    const { getByRole } = render(Harness, { "aria-label": "X", class: "custom-marker" });
    expect(getByRole("button").className).toContain("custom-marker");
  });
});
