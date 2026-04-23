import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/AlertHarness.svelte";

describe("Alert", () => {
  it("renders body text", () => {
    const { getByText } = render(Harness, { intent: "info", body: "Heads up" });
    expect(getByText("Heads up")).toBeTruthy();
  });

  it("renders the optional title above the body", () => {
    const { getByText } = render(Harness, {
      intent: "info",
      title: "Take note",
      body: "details",
    });
    expect(getByText("Take note")).toBeTruthy();
    expect(getByText("details")).toBeTruthy();
  });

  it.each([
    ["info", "status"],
    ["success", "status"],
    ["warning", "status"],
    ["danger", "alert"],
    ["attention", "alert"],
  ] as const)("uses role=%s for %s intent", (intent, expectedRole) => {
    const { getByRole } = render(Harness, { intent });
    expect(getByRole(expectedRole)).toBeTruthy();
  });

  it("does not render a dismiss button by default", () => {
    const { queryByRole } = render(Harness, { intent: "info" });
    expect(queryByRole("button", { name: "Dismiss" })).toBeNull();
  });

  it("renders a dismiss button when dismissible is true", () => {
    const { getByRole } = render(Harness, { intent: "info", dismissible: true });
    expect(getByRole("button", { name: "Dismiss" })).toBeTruthy();
  });

  it("calls onDismiss when the dismiss button is clicked", async () => {
    const onDismiss = vi.fn();
    const { getByRole } = render(Harness, {
      intent: "info",
      dismissible: true,
      onDismiss,
    });
    await fireEvent.click(getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("merges extra class prop onto the root", () => {
    const { getByRole } = render(Harness, { intent: "info", class: "custom-marker" });
    expect(getByRole("status").className).toContain("custom-marker");
  });
});
