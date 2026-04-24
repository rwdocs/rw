import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/ButtonHarness.svelte";

describe("Button", () => {
  it("renders children text", () => {
    const { getByRole } = render(Harness, { label: "Save" });
    expect(getByRole("button", { name: "Save" })).toBeTruthy();
  });

  it("defaults to primary variant (solid accent background)", () => {
    const { getByRole } = render(Harness);
    expect(getByRole("button").className).toContain("bg-accent-bg");
  });

  it("applies secondary variant classes", () => {
    const { getByRole } = render(Harness, { variant: "secondary" });
    const cls = getByRole("button").className;
    expect(cls).toContain("bg-bg-raised");
    expect(cls).toContain("border-border-default");
  });

  it("applies ghost variant classes (transparent background)", () => {
    const { getByRole } = render(Harness, { variant: "ghost" });
    expect(getByRole("button").className).toContain("bg-transparent");
  });

  it("applies danger variant classes (solid danger background)", () => {
    const { getByRole } = render(Harness, { variant: "danger" });
    expect(getByRole("button").className).toContain("bg-danger-bg-solid");
  });

  it("defaults to md size (text-sm)", () => {
    const { getByRole } = render(Harness);
    expect(getByRole("button").className).toContain("text-sm");
  });

  it("applies sm size classes (text-xs)", () => {
    const { getByRole } = render(Harness, { size: "sm" });
    expect(getByRole("button").className).toContain("text-xs");
  });

  it("applies xs icon size classes (size-5 + rounded-sm)", () => {
    const { getByRole } = render(Harness, { size: "xs", iconOnly: true });
    const cls = getByRole("button").className;
    expect(cls).toContain("size-5");
    expect(cls).toContain("rounded-sm");
  });

  it("sm size uses rounded-md", () => {
    const { getByRole } = render(Harness, { size: "sm" });
    expect(getByRole("button").className).toContain("rounded-md");
  });

  it("md size uses rounded-md", () => {
    const { getByRole } = render(Harness, { size: "md" });
    expect(getByRole("button").className).toContain("rounded-md");
  });

  it("iconOnly renders a square (width == height via size-* utility)", () => {
    const { getByRole } = render(Harness, { iconOnly: true });
    expect(getByRole("button").className).toMatch(/\bsize-\d+\b/);
  });

  it("sets aria-disabled and blocks onclick when disabled", async () => {
    const onclick = vi.fn();
    const { getByRole } = render(Harness, { disabled: true, onclick });
    const button = getByRole("button");
    expect(button.getAttribute("aria-disabled")).toBe("true");
    await fireEvent.click(button);
    expect(onclick).not.toHaveBeenCalled();
  });

  it("suppresses variant hover background while disabled", () => {
    // Regression guard: without aria-disabled:hover:bg-transparent the ghost
    // variant's hover:bg-bg-subtle still fires on disabled buttons, so a
    // disabled prev/next nav button would light up on hover.
    const { getByRole } = render(Harness, { variant: "ghost", disabled: true });
    expect(getByRole("button").className).toContain("aria-disabled:hover:bg-transparent");
  });

  it("sets aria-disabled and blocks onclick when loading", async () => {
    const onclick = vi.fn();
    const { getByRole } = render(Harness, { loading: true, onclick });
    const button = getByRole("button");
    expect(button.getAttribute("aria-disabled")).toBe("true");
    await fireEvent.click(button);
    expect(onclick).not.toHaveBeenCalled();
  });

  it("calls onclick when enabled and clicked", async () => {
    const onclick = vi.fn();
    const { getByRole } = render(Harness, { onclick });
    await fireEvent.click(getByRole("button"));
    expect(onclick).toHaveBeenCalledTimes(1);
  });

  it("renders as a native <button> (inherits keyboard activation)", () => {
    // The spec requires keyboard activation. We deliver that by using a real
    // <button> element rather than <div role="button">, so Enter/Space dispatch
    // click natively. Asserting the tag is the honest, jsdom-independent check.
    const { getByRole } = render(Harness);
    expect(getByRole("button").tagName).toBe("BUTTON");
  });

  it("merges extra class prop", () => {
    const { getByRole } = render(Harness, { class: "custom-marker" });
    expect(getByRole("button").className).toContain("custom-marker");
  });
});
