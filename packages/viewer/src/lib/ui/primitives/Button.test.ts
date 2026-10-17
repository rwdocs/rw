import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/ButtonHarness.svelte";

describe("Button", () => {
  it("renders children text", () => {
    const { getByRole } = render(Harness, { label: "Save" });
    expect(getByRole("button", { name: "Save" })).toBeTruthy();
  });

  it.each([
    { name: "primary variant by default", props: {}, classes: ["bg-accent-bg"] },
    {
      name: "secondary variant",
      props: { variant: "secondary" },
      classes: ["bg-bg-raised", "border-border-default"],
    },
    { name: "ghost variant", props: { variant: "ghost" }, classes: ["bg-transparent"] },
    { name: "danger variant", props: { variant: "danger" }, classes: ["bg-danger-bg-solid"] },
    { name: "md size by default", props: {}, classes: ["text-sm"] },
    { name: "md size", props: { size: "md" }, classes: ["rounded-md"] },
    { name: "sm size", props: { size: "sm" }, classes: ["text-xs", "rounded-md"] },
    {
      name: "xs icon size",
      props: { size: "xs", iconOnly: true },
      classes: ["size-5", "rounded-sm"],
    },
  ] as const)("applies $name classes", ({ props, classes }) => {
    const cls = render(Harness, props).getByRole("button").className;
    for (const c of classes) expect(cls).toContain(c);
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

  it.each(["primary", "secondary", "ghost", "danger"] as const)(
    "gates the %s hover background on not being disabled",
    (variant) => {
      // jsdom computes no Tailwind styles; e2e/button-hover.spec.ts checks the result.
      const cls = render(Harness, { variant, disabled: true }).getByRole("button").className;
      expect(cls).toContain("not-aria-disabled:hover:bg-");
      expect(cls).not.toMatch(/(^|\s)(aria-disabled:)?hover:bg-/);
    },
  );

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
