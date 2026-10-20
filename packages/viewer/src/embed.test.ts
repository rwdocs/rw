import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mountRw } from "./embed";
import type {
  CommentApiClient,
  Comment,
  CreateCommentRequest,
  UpdateCommentRequest,
  Author,
  Selector,
  CommentStatus,
} from "./embed";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

// `svelte-check` fails if any public type is dropped from the package entry.
// Exported so it is not flagged as unused.
export type _PublicTypeSurface = [
  CommentApiClient,
  Comment,
  CreateCommentRequest,
  UpdateCommentRequest,
  Author,
  Selector,
  CommentStatus,
];

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
      basePath: "",
      location: "/",
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
