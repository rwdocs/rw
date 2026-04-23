import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/BadgeHarness.svelte";

describe("Badge", () => {
  it("renders body text", () => {
    const { getByText } = render(Harness, { body: "beta" });
    expect(getByText("beta")).toBeTruthy();
  });

  it("merges extra class prop onto the root", () => {
    const { getByText } = render(Harness, { body: "x", class: "italic" });
    expect(getByText("x").className).toContain("italic");
  });

  it("forwards HTML attributes like title onto the root span", () => {
    const { getByText } = render(Harness, { body: "x", title: "tooltip" });
    expect(getByText("x").getAttribute("title")).toBe("tooltip");
  });

  it("renders as a <span>", () => {
    const { getByText } = render(Harness, { body: "x" });
    expect(getByText("x").tagName).toBe("SPAN");
  });
});
