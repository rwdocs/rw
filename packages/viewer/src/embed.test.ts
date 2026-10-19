import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mountRw } from "./embed";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const pendingFetch = (() => new Promise<Response>(() => {})) as typeof fetch;

describe("mountRw", () => {
  it("leaves the target as it found it on destroy", () => {
    const target = document.createElement("div");
    const instance = mountRw(target, {
      apiBaseUrl: "/_api",
      sectionRef: "system:default/a",
      fetchFn: pendingFetch,
      colorScheme: "dark",
    });
    expect(target.classList.contains("dark")).toBe(true);

    instance.destroy();

    expect(target.classList.contains("dark")).toBe(false);
    expect(target.hasAttribute("data-rw-viewer")).toBe(false);
  });
});
