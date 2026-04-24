import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import Avatar from "./Avatar.svelte";

function root(container: HTMLElement): HTMLElement {
  return container.firstElementChild as HTMLElement;
}

describe("Avatar", () => {
  it("renders an <img> when src is provided, regardless of variant", () => {
    const { container } = render(Avatar, {
      variant: "initials",
      src: "https://example.com/a.png",
      name: "Jane Doe",
    });
    const el = root(container);
    expect(el.tagName).toBe("IMG");
    expect(el.getAttribute("src")).toBe("https://example.com/a.png");
    // Decorative: callers render the author's name as visible text alongside the avatar,
    // so an informative alt would cause screen readers to announce the name twice.
    expect(el.getAttribute("alt")).toBe("");
  });

  it("renders the person icon (SVG in a span) for variant=person", () => {
    const { container } = render(Avatar, { variant: "person" });
    const el = root(container);
    expect(el.tagName).toBe("SPAN");
    expect(el.getAttribute("aria-hidden")).toBe("true");
    expect(el.querySelector("svg")).toBeTruthy();
  });

  it("renders the sparkles icon (SVG in a span) for variant=ai", () => {
    const { container } = render(Avatar, { variant: "ai" });
    const el = root(container);
    expect(el.tagName).toBe("SPAN");
    expect(el.querySelector("svg")).toBeTruthy();
  });

  it("renders two-letter initials uppercased for multi-token names", () => {
    const { container } = render(Avatar, { variant: "initials", name: "Jane Doe" });
    expect(root(container).textContent?.trim()).toBe("JD");
  });

  it("renders one-letter initial for single-token names", () => {
    const { container } = render(Avatar, { variant: "initials", name: "alice" });
    expect(root(container).textContent?.trim()).toBe("A");
  });

  it("falls back to ? when name is empty or whitespace", () => {
    const { container } = render(Avatar, { variant: "initials", name: "   " });
    expect(root(container).textContent?.trim()).toBe("?");
  });

  it("applies size to width and height via inline style", () => {
    const { container } = render(Avatar, { variant: "person", size: 32 });
    const el = root(container);
    expect(el.style.width).toBe("32px");
    expect(el.style.height).toBe("32px");
  });

  it("merges extra class prop onto the root", () => {
    const { container } = render(Avatar, { variant: "initials", name: "A", class: "ring-2" });
    expect(root(container).className).toContain("ring-2");
  });

  it("uses semantic surface/foreground tokens (not raw gray utilities)", () => {
    // Regression guard: Avatar must read from the token layer so light/dark
    // switching and future palette changes do not require edits here.
    const { container } = render(Avatar, { variant: "person" });
    const cls = root(container).className;
    expect(cls).toContain("bg-bg-subtle");
    expect(cls).toContain("text-fg-muted");
  });
});
