import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/ButtonGroupHarness.svelte";

describe("ButtonGroup", () => {
  it("renders a group landmark with its accessible name and the child buttons", () => {
    const { getByRole } = render(Harness, { props: { label: "Zoom controls" } });
    const group = getByRole("group", { name: "Zoom controls" });
    expect(group).toBeTruthy();
    // Children are rendered in order as direct button descendants.
    const buttons = group.querySelectorAll("button");
    expect(buttons.length).toBe(3);
    expect(buttons[0].getAttribute("aria-label")).toBe("Zoom out");
    expect(buttons[2].getAttribute("aria-label")).toBe("Zoom in");
  });
});
