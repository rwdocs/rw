import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import TocSidebar from "./TocSidebar.svelte";
import type { TocEntry } from "../types";

const noop = () => {};

const toc: TocEntry[] = [
  { id: "features", title: "Features", level: 2 },
  { id: "quick-start", title: "Quick Start", level: 2 },
];

describe("TocSidebar aria-current", () => {
  it('marks the active entry with aria-current="true"', () => {
    const { getByRole } = render(TocSidebar, { toc, activeId: "quick-start", onNavigate: noop });
    expect(getByRole("link", { name: "Quick Start" }).getAttribute("aria-current")).toBe("true");
  });

  it("omits aria-current on inactive entries", () => {
    const { getByRole } = render(TocSidebar, { toc, activeId: "quick-start", onNavigate: noop });
    expect(getByRole("link", { name: "Features" }).getAttribute("aria-current")).toBeNull();
  });

  it("omits aria-current on every entry when nothing is active", () => {
    const { getByRole } = render(TocSidebar, { toc, activeId: null, onNavigate: noop });
    expect(getByRole("link", { name: "Features" }).getAttribute("aria-current")).toBeNull();
    expect(getByRole("link", { name: "Quick Start" }).getAttribute("aria-current")).toBeNull();
  });
});
