import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/svelte";
import Harness from "./__fixtures__/KitHarness.svelte";

const EXPECTED_SECTIONS: ReadonlyArray<readonly [id: string, title: string]> = [
  ["buttons", "Buttons"],
  ["icon-buttons", "Icon Buttons"],
  ["badges", "Badges"],
  ["avatars", "Avatars"],
  ["alerts", "Alerts"],
  ["popover", "Popover"],
  ["menu", "Menu"],
  ["loading-bar", "LoadingBar"],
  ["loading-skeleton", "LoadingSkeleton"],
  ["quote", "Quote"],
];

describe("Kit page", () => {
  it("renders the page heading", () => {
    const { getByRole } = render(Harness);
    expect(getByRole("heading", { level: 1, name: "Design Kit" })).toBeTruthy();
  });

  it("renders every primitive section with a stable anchor id", () => {
    const { getByRole } = render(Harness);
    for (const [id, title] of EXPECTED_SECTIONS) {
      const heading = getByRole("heading", { level: 2, name: title });
      expect(heading.id, `${title} heading id`).toBe(id);
    }
  });

  it("clears prior page data on mount", () => {
    const clear = vi.fn();
    render(Harness, { props: { clear } });
    expect(clear).toHaveBeenCalledTimes(1);
  });
});
