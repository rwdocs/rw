import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/IconButtonHarness.svelte";

describe("IconButton", () => {
  it("renders a button with the given aria-label", () => {
    const { getByRole } = render(Harness, { "aria-label": "Open menu" });
    expect(getByRole("button", { name: "Open menu" })).toBeTruthy();
  });

  it("applies pressed-style classes when active", () => {
    const { getByRole } = render(Harness, { "aria-label": "X", active: true });
    const cls = getByRole("button").className;
    expect(cls).toContain("border-border-strong");
  });

  it("defaults to type=button so it never submits an enclosing form", () => {
    const { getByRole } = render(Harness, { "aria-label": "X" });
    expect(getByRole("button").getAttribute("type")).toBe("button");
  });

  it("lets callers override the button type", () => {
    const { getByRole } = render(Harness, { "aria-label": "X", type: "submit" });
    expect(getByRole("button").getAttribute("type")).toBe("submit");
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
});
