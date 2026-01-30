import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";
import type { NavigationTree } from "../types";

// Mock the API client
vi.mock("../api/client", () => ({
  fetchNavigation: vi.fn(),
}));

import { navigation, collectParentPaths, getParentPaths } from "./navigation";
import { fetchNavigation } from "../api/client";

const mockFetchNavigation = vi.mocked(fetchNavigation);

const mockTree: NavigationTree = {
  items: [
    { title: "Home", path: "/" },
    {
      title: "Guide",
      path: "/guide",
      children: [
        { title: "Getting Started", path: "/guide/getting-started" },
        {
          title: "Advanced",
          path: "/guide/advanced",
          children: [{ title: "Plugins", path: "/guide/advanced/plugins" }],
        },
      ],
    },
    { title: "API", path: "/api" },
  ],
  scope: null,
  parentScope: null,
};

describe("collectParentPaths", () => {
  it("returns empty array for items without children", () => {
    const items = [
      { title: "Home", path: "/" },
      { title: "About", path: "/about" },
    ];
    expect(collectParentPaths(items)).toEqual([]);
  });

  it("collects paths of items with children", () => {
    const items = [
      { title: "Home", path: "/" },
      {
        title: "Guide",
        path: "/guide",
        children: [{ title: "Getting Started", path: "/guide/getting-started" }],
      },
    ];
    expect(collectParentPaths(items)).toEqual(["/guide"]);
  });

  it("recursively collects nested parent paths", () => {
    const paths = collectParentPaths(mockTree.items);
    expect(paths).toContain("/guide");
    expect(paths).toContain("/guide/advanced");
    expect(paths).toHaveLength(2);
  });
});

describe("getParentPaths", () => {
  it("returns empty array for root path", () => {
    expect(getParentPaths("/")).toEqual([]);
  });

  it("returns single path for top-level item", () => {
    expect(getParentPaths("/guide")).toEqual(["/guide"]);
  });

  it("returns all ancestor paths for nested item", () => {
    expect(getParentPaths("/guide/advanced/plugins")).toEqual([
      "/guide",
      "/guide/advanced",
      "/guide/advanced/plugins",
    ]);
  });
});

describe("navigation store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    navigation._reset();
  });

  describe("load", () => {
    it("fetches navigation tree and updates state", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);

      await navigation.load();

      const state = get(navigation);
      expect(state.tree).toEqual(mockTree);
      expect(state.loading).toBe(false);
      expect(state.error).toBeNull();
    });

    it("collapses all parent items by default", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);

      await navigation.load();

      const state = get(navigation);
      // /guide and /guide/advanced have children, so they should be collapsed
      expect(state.collapsed.has("/guide")).toBe(true);
      expect(state.collapsed.has("/guide/advanced")).toBe(true);
      // Items without children should not be in collapsed set
      expect(state.collapsed.has("/")).toBe(false);
      expect(state.collapsed.has("/api")).toBe(false);
    });

    it("sets error on fetch failure", async () => {
      mockFetchNavigation.mockRejectedValue(new Error("Network error"));

      await navigation.load();

      const state = get(navigation);
      expect(state.tree).toBeNull();
      expect(state.loading).toBe(false);
      expect(state.error).toBe("Network error");
    });

    it("handles non-Error exceptions", async () => {
      mockFetchNavigation.mockRejectedValue("String error");

      await navigation.load();

      const state = get(navigation);
      expect(state.error).toBe("Unknown error");
    });

    it("passes bypassCache option to fetch", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);

      await navigation.load({ bypassCache: true });

      expect(mockFetchNavigation).toHaveBeenCalledWith(
        expect.objectContaining({ bypassCache: true }),
      );
    });

    it("aborts previous request when new load starts", async () => {
      let firstResolve: (value: NavigationTree) => void;
      const firstPromise = new Promise<NavigationTree>((resolve) => {
        firstResolve = resolve;
      });

      const secondTree: NavigationTree = {
        items: [{ path: "/second", title: "Second", children: [] }],
        scope: null,
        parentScope: null,
      };

      mockFetchNavigation
        .mockImplementationOnce(() => firstPromise)
        .mockResolvedValueOnce(secondTree);

      // Start first request (don't await)
      void navigation.loadScope("first");

      // Start second request before first completes
      const secondLoad = navigation.loadScope("second");

      // First request's signal should be aborted
      const firstCall = mockFetchNavigation.mock.calls[0]?.[0] as { signal: AbortSignal };
      expect(firstCall.signal.aborted).toBe(true);

      // Second request's signal should not be aborted
      const secondCall = mockFetchNavigation.mock.calls[1]?.[0] as { signal: AbortSignal };
      expect(secondCall.signal.aborted).toBe(false);

      // Resolve first request (should be ignored)
      firstResolve!(mockTree);
      await secondLoad;

      // State should have second tree, not first
      const state = get(navigation);
      expect(state.tree).toEqual(secondTree);
      expect(state.currentScope).toBe("second");
    });

    it("silently ignores AbortError", async () => {
      const abortError = new DOMException("Aborted", "AbortError");
      mockFetchNavigation.mockRejectedValue(abortError);

      await navigation.load();

      const state = get(navigation);
      // Should not set error for AbortError
      expect(state.error).toBeNull();
      // Should still be loading since we didn't complete
      expect(state.loading).toBe(true);
    });
  });

  describe("toggle", () => {
    it("expands a collapsed item", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      await navigation.load();

      navigation.toggle("/guide");

      const state = get(navigation);
      expect(state.collapsed.has("/guide")).toBe(false);
    });

    it("collapses an expanded item", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      await navigation.load();

      // First expand
      navigation.toggle("/guide");
      expect(get(navigation).collapsed.has("/guide")).toBe(false);

      // Then collapse
      navigation.toggle("/guide");
      expect(get(navigation).collapsed.has("/guide")).toBe(true);
    });
  });

  describe("expandOnlyTo", () => {
    it("expands path to target and collapses others", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      await navigation.load();

      navigation.expandOnlyTo("/guide/advanced/plugins");

      const state = get(navigation);
      // Path to target should be expanded
      expect(state.collapsed.has("/guide")).toBe(false);
      expect(state.collapsed.has("/guide/advanced")).toBe(false);
    });

    it("does nothing if tree is not loaded", () => {
      // Don't load the tree, store is in reset state
      navigation.expandOnlyTo("/guide");

      const state = get(navigation);
      expect(state.tree).toBeNull();
    });

    it("skips update if path is already expanded", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      await navigation.load();

      // Expand to a path
      navigation.expandOnlyTo("/guide/getting-started");

      // Get initial state
      const stateBefore = get(navigation);

      // Expand to same path again - should be a no-op
      navigation.expandOnlyTo("/guide/getting-started");

      const stateAfter = get(navigation);
      // State reference should be the same (no update)
      expect(stateAfter).toBe(stateBefore);
    });
  });
});
