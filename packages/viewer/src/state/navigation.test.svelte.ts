import { describe, it, expect, vi, beforeEach } from "vitest";
import type { NavigationTree } from "../types";
import { Navigation, collectParentPaths, getParentPaths } from "./navigation.svelte";
import type { ApiClient } from "../api/client";

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
};

function createMockApiClient(overrides: Record<string, ReturnType<typeof vi.fn>> = {}): ApiClient {
  return {
    fetchConfig: vi.fn(),
    fetchPage: vi.fn(),
    fetchNavigation: vi.fn(),
    ...overrides,
  } as unknown as ApiClient;
}

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
  let mockFetchNavigation: ReturnType<typeof vi.fn>;
  let mockApiClient: ApiClient;

  beforeEach(() => {
    vi.clearAllMocks();
    mockFetchNavigation = vi.fn();
    mockApiClient = createMockApiClient({ fetchNavigation: mockFetchNavigation });
  });

  describe("load", () => {
    it("fetches navigation tree and updates state", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);

      await navigation.load();

      expect(navigation.tree).toEqual(mockTree);
      expect(navigation.loading).toBe(false);
      expect(navigation.error).toBeNull();
    });

    it("collapses all parent items by default", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);

      await navigation.load();

      // /guide and /guide/advanced have children, so they should be collapsed
      expect(navigation.collapsed.has("/guide")).toBe(true);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(true);
      // Items without children should not be in collapsed set
      expect(navigation.collapsed.has("/")).toBe(false);
      expect(navigation.collapsed.has("/api")).toBe(false);
    });

    it("sets error on fetch failure", async () => {
      mockFetchNavigation.mockRejectedValue(new Error("Network error"));
      const navigation = new Navigation(mockApiClient);

      await navigation.load();

      expect(navigation.tree).toBeNull();
      expect(navigation.loading).toBe(false);
      expect(navigation.error).toBe("Network error");
    });

    it("handles non-Error exceptions", async () => {
      mockFetchNavigation.mockRejectedValue("String error");
      const navigation = new Navigation(mockApiClient);

      await navigation.load();

      expect(navigation.error).toBe("Unknown error");
    });

    it("passes bypassCache option to fetch", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);

      await navigation.load({ bypassCache: true });

      expect(mockFetchNavigation).toHaveBeenCalledWith(
        expect.objectContaining({ bypassCache: true }),
      );
    });

    it("forwards sectionRef to fetchNavigation", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);

      await navigation.load({ sectionRef: "domain:default/billing" });

      expect(mockFetchNavigation).toHaveBeenCalledWith(
        expect.objectContaining({ sectionRef: "domain:default/billing" }),
      );
      expect(navigation.currentSectionRef).toBe("domain:default/billing");
    });

    it("aborts previous request when new load starts", async () => {
      let firstResolve: (value: NavigationTree) => void;
      const firstPromise = new Promise<NavigationTree>((resolve) => {
        firstResolve = resolve;
      });

      const secondTree: NavigationTree = {
        items: [{ path: "/second", title: "Second", children: [] }],
      };

      mockFetchNavigation
        .mockImplementationOnce(() => firstPromise)
        .mockResolvedValueOnce(secondTree);

      const navigation = new Navigation(mockApiClient);

      // Start first request (don't await)
      void navigation.loadSection("first");

      // Start second request before first completes
      const secondLoad = navigation.loadSection("second");

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
      expect(navigation.tree).toEqual(secondTree);
      expect(navigation.currentSectionRef).toBe("second");
    });

    it("silently ignores AbortError", async () => {
      const abortError = new DOMException("Aborted", "AbortError");
      mockFetchNavigation.mockRejectedValue(abortError);
      const navigation = new Navigation(mockApiClient);

      await navigation.load();

      // Should not set error for AbortError
      expect(navigation.error).toBeNull();
      // Should still be loading since we didn't complete
      expect(navigation.loading).toBe(true);
    });
  });

  describe("toggle", () => {
    it("expands a collapsed item", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      navigation.toggle("/guide");

      expect(navigation.collapsed.has("/guide")).toBe(false);
    });

    it("collapses an expanded item", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      // First expand
      navigation.toggle("/guide");
      expect(navigation.collapsed.has("/guide")).toBe(false);

      // Then collapse
      navigation.toggle("/guide");
      expect(navigation.collapsed.has("/guide")).toBe(true);
    });
  });

  describe("expandOnlyTo", () => {
    it("expands path to target and collapses others", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      navigation.expandOnlyTo("/guide/advanced/plugins");

      // Path to target should be expanded
      expect(navigation.collapsed.has("/guide")).toBe(false);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);
    });

    it("does nothing if tree is not loaded", () => {
      const navigation = new Navigation(mockApiClient);
      // Don't load the tree, store is in reset state
      navigation._reset();
      navigation.expandOnlyTo("/guide");

      expect(navigation.tree).toBeNull();
    });

    it("skips update if path is already expanded", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      // Expand to a path
      navigation.expandOnlyTo("/guide/getting-started");

      // Get collapsed state before
      const collapsedBefore = new Set(navigation.collapsed);

      // Expand to same path again - should be a no-op
      navigation.expandOnlyTo("/guide/getting-started");

      const collapsedAfter = navigation.collapsed;
      // Same contents
      expect(collapsedAfter.size).toBe(collapsedBefore.size);
      for (const p of collapsedBefore) {
        expect(collapsedAfter.has(p)).toBe(true);
      }
    });

    it("re-expands active path after loadSection replaces the tree", async () => {
      const scopedTree: NavigationTree = {
        items: [
          {
            title: "Billing",
            path: "/billing",
            children: [
              { title: "Payments", path: "/billing/payments" },
              { title: "Invoices", path: "/billing/invoices" },
            ],
          },
        ],
      };

      mockFetchNavigation.mockResolvedValue(scopedTree);
      const navigation = new Navigation(mockApiClient);

      // Set active path before tree is loaded (simulates Page.svelte subscription)
      navigation.expandOnlyTo("/billing/payments");

      // Load navigation (simulates sectionWatcher calling loadSection)
      await navigation.loadSection("billing");

      // /billing should be expanded (ancestor of active path)
      expect(navigation.collapsed.has("/billing")).toBe(false);
    });

    it("re-expands active path when scope changes after initial load", async () => {
      const scopedTree: NavigationTree = {
        items: [
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
        ],
      };

      mockFetchNavigation.mockResolvedValueOnce(mockTree).mockResolvedValueOnce(scopedTree);
      const navigation = new Navigation(mockApiClient);

      // Initial load and expand (simulates Layout.svelte)
      await navigation.load();
      navigation.expandOnlyTo("/guide/advanced/plugins");

      // Verify expanded
      expect(navigation.collapsed.has("/guide")).toBe(false);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);

      // Section change replaces the tree (simulates sectionWatcher)
      await navigation.loadSection("guide");

      // Active path should still be expanded in the new tree
      expect(navigation.collapsed.has("/guide")).toBe(false);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);
    });
  });
});
