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

  it.each([
    {
      name: "renders all three parts in order without stray separators",
      props: { prefix: "before ", exact: "exact", suffix: " after" },
      expected: "…before exact after…",
    },
    {
      name: "omits prefix and its ellipsis when prefix is absent",
      props: { exact: "exact", suffix: " after" },
      expected: "exact after…",
    },
    {
      name: "omits suffix and its ellipsis when suffix is absent",
      props: { prefix: "before ", exact: "exact" },
      expected: "…before exact",
    },
    {
      name: "renders only the exact text when both prefix and suffix are absent",
      props: { exact: "exact" },
      expected: "exact",
    },
  ])("$name", ({ props, expected }) => {
    const { container } = render(Quote, props);
    expect(text(container.querySelector("blockquote")!)).toBe(expected);
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
