import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import Quote from "./Quote.svelte";

// Normalize insignificant whitespace (template indentation between the
// opening <blockquote> tag and its first child) so assertions focus on
// visible text rather than source formatting.
const text = (node: Element) => node.textContent?.replace(/\s+/g, " ").trim() ?? "";

describe("Quote", () => {
  it("renders the exact passage inside a <mark>", () => {
    const { container } = render(Quote, { exact: "the exact bit" });
    const mark = container.querySelector("mark");
    expect(mark?.textContent).toBe("the exact bit");
  });

  it("renders all three parts in order without stray separators", () => {
    const { container } = render(Quote, {
      prefix: "before ",
      exact: "exact",
      suffix: " after",
    });
    const bq = container.querySelector("blockquote")!;
    expect(text(bq)).toBe("…before exact after…");
  });

  it("omits prefix and its ellipsis when prefix is absent", () => {
    const { container } = render(Quote, { exact: "exact", suffix: " after" });
    const bq = container.querySelector("blockquote")!;
    expect(text(bq)).toBe("exact after…");
  });

  it("omits suffix and its ellipsis when suffix is absent", () => {
    const { container } = render(Quote, { prefix: "before ", exact: "exact" });
    const bq = container.querySelector("blockquote")!;
    expect(text(bq)).toBe("…before exact");
  });

  it("renders only the exact text when both prefix and suffix are absent", () => {
    const { container } = render(Quote, { exact: "exact" });
    const bq = container.querySelector("blockquote")!;
    expect(text(bq)).toBe("exact");
  });

  it("renders as a <blockquote>", () => {
    const { container } = render(Quote, { exact: "x" });
    expect(container.querySelector("blockquote")).toBeTruthy();
  });

  it("merges extra class prop onto the root", () => {
    const { container } = render(Quote, { exact: "x", class: "mb-4" });
    expect(container.querySelector("blockquote")?.className).toContain("mb-4");
  });

  it("forwards HTML attributes like title onto the root", () => {
    const { container } = render(Quote, { exact: "x", title: "tooltip" });
    expect(container.querySelector("blockquote")?.getAttribute("title")).toBe("tooltip");
  });
});
