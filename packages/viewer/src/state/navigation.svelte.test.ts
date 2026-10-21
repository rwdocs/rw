import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import type { Mock } from "vitest";
import type { NavigationTree, NavItem, SectionInfo } from "../types";
import { SectionLinks, sectionRefString } from "$lib/sectionRefs";
import { Navigation, collectParentPaths, getParentPaths } from "./navigation.svelte";
import { Router } from "./router.svelte";
import type { ApiClient } from "../api/client";

// Torn down here rather than at the end of the test body: a failing assertion
// would otherwise leak the root effect into later tests.
let teardown: (() => void) | null = null;

afterEach(() => {
  teardown?.();
  teardown = null;
});

const rootSection: SectionInfo = { kind: "section", namespace: "default", name: "root" };
const billingSection: SectionInfo = { kind: "domain", namespace: "default", name: "billing" };
const guideSection: SectionInfo = { kind: "guide", namespace: "default", name: "guide" };
const ROOT_REF = sectionRefString(rootSection);
const BILLING_REF = sectionRefString(billingSection);

/** A tree as the server sends it for the section rooted at `path`. */
function scopedTree(section: SectionInfo, path: string, items: NavItem[]): NavigationTree {
  const ref = sectionRefString(section);
  const chain = [{ sectionRef: ref, subpath: "" }];
  if (ref !== ROOT_REF) chain.push({ sectionRef: ROOT_REF, subpath: path.slice(1) });
  return {
    items,
    scope: { path, title: section.name, section },
    sectionAncestry: { [ref]: chain },
  };
}

const mockTree = scopedTree(rootSection, "/", [
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
]);

function createMockApiClient(overrides: Record<string, ReturnType<typeof vi.fn>> = {}): ApiClient {
  return {
    fetchConfig: vi.fn(),
    fetchPage: vi.fn(),
    fetchNavigation: vi.fn(),
    ...overrides,
  };
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
  it.each([
    ["/", []],
    ["/guide", ["/guide"]],
    ["/guide/advanced/plugins", ["/guide", "/guide/advanced", "/guide/advanced/plugins"]],
  ])("maps %j to %j", (path, expected) => {
    expect(getParentPaths(path)).toEqual(expected);
  });
});

describe("navigation store", () => {
  let mockFetchNavigation: Mock<ApiClient["fetchNavigation"]>;
  let mockApiClient: ApiClient;

  beforeEach(() => {
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

      await navigation.load(undefined, { bypassCache: true });

      expect(mockFetchNavigation).toHaveBeenCalledWith(
        expect.objectContaining({ bypassCache: true }),
      );
    });

    it("forwards sectionRef to fetchNavigation", async () => {
      mockFetchNavigation.mockResolvedValue(scopedTree(billingSection, "/billing", []));
      const navigation = new Navigation(mockApiClient);

      await navigation.load(BILLING_REF);

      expect(mockFetchNavigation).toHaveBeenCalledWith(
        expect.objectContaining({ sectionRef: BILLING_REF }),
      );
      expect(navigation.currentSectionRef).toBe(BILLING_REF);
    });

    it("aborts previous request when new load starts", async () => {
      let firstResolve: (value: NavigationTree) => void;
      const firstPromise = new Promise<NavigationTree>((resolve) => {
        firstResolve = resolve;
      });

      const secondTree = scopedTree(billingSection, "/billing", [
        { path: "/billing/second", title: "Second", children: [] },
      ]);

      mockFetchNavigation
        .mockImplementationOnce(() => firstPromise)
        .mockResolvedValueOnce(secondTree);

      const navigation = new Navigation(mockApiClient);

      // Start first request (don't await)
      void navigation.load(ROOT_REF);

      // Start second request before first completes
      const secondLoad = navigation.load(BILLING_REF);

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
      expect(navigation.currentSectionRef).toBe(BILLING_REF);
    });

    it("surfaces an AbortError raised by the host fetch itself (e.g. its own timeout)", async () => {
      mockFetchNavigation.mockRejectedValue(new DOMException("Aborted", "AbortError"));
      const navigation = new Navigation(mockApiClient);

      await navigation.load();

      expect(navigation.loading).toBe(false);
      // Not the message: jsdom's DOMException, unlike a browser's, is no Error.
      expect(navigation.error).not.toBeNull();
    });

    it("ignores the AbortError of a load it superseded", async () => {
      const secondTree = scopedTree(billingSection, "/billing", [
        { path: "/billing/second", title: "Second", children: [] },
      ]);
      mockFetchNavigation
        .mockImplementationOnce(
          (options?: { signal?: AbortSignal }) =>
            new Promise<NavigationTree>((_, reject) => {
              options?.signal?.addEventListener("abort", () =>
                reject(new DOMException("Aborted", "AbortError")),
              );
            }),
        )
        .mockResolvedValueOnce(secondTree);
      const navigation = new Navigation(mockApiClient);

      const firstLoad = navigation.load(ROOT_REF);
      await navigation.load(BILLING_REF);
      await firstLoad;

      expect(navigation.tree).toEqual(secondTree);
      expect(navigation.loading).toBe(false);
      expect(navigation.error).toBeNull();
    });

    it("ignores a superseded non-abort rejection (does not flip sidebar into error)", async () => {
      let rejectFirst: (e: Error) => void;
      const firstPromise = new Promise<NavigationTree>((_, reject) => {
        rejectFirst = reject;
      });
      const secondTree = scopedTree(billingSection, "/billing", [
        { path: "/billing/second", title: "Second", children: [] },
      ]);

      mockFetchNavigation
        .mockImplementationOnce(() => firstPromise)
        .mockResolvedValueOnce(secondTree);

      const navigation = new Navigation(mockApiClient);

      // First load hangs; second supersedes it (aborting it) and succeeds.
      const firstLoad = navigation.load(ROOT_REF);
      await navigation.load(BILLING_REF);
      expect(navigation.tree).toEqual(secondTree);
      expect(navigation.error).toBeNull();

      // The superseded first nav now rejects with a PLAIN Error (not AbortError).
      rejectFirst!(new Error("network blip"));
      await firstLoad;

      // The winning load's state must be intact — no error, no spurious loading.
      expect(navigation.tree).toEqual(secondTree);
      expect(navigation.error).toBeNull();
      expect(navigation.loading).toBe(false);
    });

    it("keeps the fetched (unresolved) tree when the section-ref resolver rejects", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const resolver = vi.fn().mockRejectedValue(new Error("host resolver failed"));
      const navigation = new Navigation(mockApiClient, new SectionLinks(resolver));

      await navigation.load();

      // The fetch succeeded; resolver rejection must not blank/error the
      // sidebar — it just falls back to the unresolved tree.
      expect(resolver).toHaveBeenCalled();
      expect(navigation.tree).toEqual(mockTree);
      expect(navigation.error).toBeNull();
      expect(navigation.loading).toBe(false);
    });

    it("still errors when the FETCH itself rejects, even with a section-ref resolver configured", async () => {
      mockFetchNavigation.mockRejectedValue(new Error("Network error"));
      const resolver = vi.fn().mockResolvedValue({});
      const navigation = new Navigation(mockApiClient, new SectionLinks(resolver));

      await navigation.load();

      expect(navigation.tree).toBeNull();
      expect(navigation.error).toBe("Network error");
      // The resolver is never reached because fetchNavigation rejected first.
      expect(resolver).not.toHaveBeenCalled();
    });
  });

  describe("sectionHref", () => {
    const BILLING_BASE = "/catalog/default/domain/billing/docs";
    const sysSection: SectionInfo = { kind: "system", namespace: "default", name: "sys#1" };
    const tree: NavigationTree = {
      items: [
        { title: "Billing", path: "/billing", section: billingSection },
        { title: "Sys", path: "/billing/sys#1", section: sysSection },
        { title: "Plain", path: "/plain" },
      ],
      scope: { path: "/billing", title: "Billing", section: billingSection },
      parentScope: { path: "/", title: "Home", section: rootSection },
      sectionAncestry: {
        [BILLING_REF]: [
          { sectionRef: BILLING_REF, subpath: "" },
          { sectionRef: ROOT_REF, subpath: "billing" },
        ],
        "system:default/sys#1": [
          { sectionRef: "system:default/sys#1", subpath: "" },
          { sectionRef: BILLING_REF, subpath: "sys#1" },
          { sectionRef: ROOT_REF, subpath: "billing/sys#1" },
        ],
        [ROOT_REF]: [{ sectionRef: ROOT_REF, subpath: "" }],
      },
    };

    async function loaded(resolved: Record<string, string>): Promise<Navigation> {
      mockFetchNavigation.mockResolvedValue(tree);
      const navigation = new Navigation(
        mockApiClient,
        new SectionLinks(() => Promise.resolve(resolved)),
      );
      await navigation.load();
      return navigation;
    }

    it("resolves section roots by their own ref, an unmapped one via its ancestor", async () => {
      const navigation = await loaded({ [BILLING_REF]: BILLING_BASE });

      expect(navigation.sectionHref(tree.items[0])).toBe(BILLING_BASE);
      expect(navigation.sectionHref(tree.items[1])).toBe(`${BILLING_BASE}/sys%231`);
      expect(navigation.sectionHref(tree.scope)).toBe(BILLING_BASE);
      expect(navigation.sectionHref(tree.parentScope!)).toBeUndefined();
    });

    it("leaves an item without a section to local routing", async () => {
      const navigation = await loaded({ [ROOT_REF]: "/host" });

      expect(navigation.sectionHref(tree.items[2])).toBeUndefined();
      expect(navigation.sectionHref(tree.parentScope!)).toBe("/host");
    });
  });

  describe("loadSection", () => {
    const rootTree = scopedTree(rootSection, "/", []);

    it("reuses an unscoped tree whose scope is the requested section", async () => {
      mockFetchNavigation.mockResolvedValue(rootTree);
      const navigation = new Navigation(mockApiClient);

      await navigation.load();
      await navigation.loadSection(ROOT_REF);

      expect(navigation.currentSectionRef).toBe(ROOT_REF);
      expect(mockFetchNavigation).toHaveBeenCalledTimes(1);
    });

    it("loads a section other than the one shown", async () => {
      mockFetchNavigation.mockResolvedValue(rootTree);
      const navigation = new Navigation(mockApiClient);

      await navigation.load();
      await navigation.loadSection(BILLING_REF);

      expect(mockFetchNavigation).toHaveBeenLastCalledWith(
        expect.objectContaining({ sectionRef: BILLING_REF }),
      );
    });

    it("supersedes a load for another section even when its own is shown", async () => {
      mockFetchNavigation.mockResolvedValueOnce(rootTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      mockFetchNavigation.mockImplementationOnce(() => new Promise(() => {}));
      void navigation.load(BILLING_REF);
      mockFetchNavigation.mockResolvedValueOnce(rootTree);
      await navigation.loadSection(ROOT_REF);

      expect(mockFetchNavigation).toHaveBeenCalledTimes(3);
    });
  });

  describe("toggle", () => {
    it("flips an item between collapsed and expanded", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      navigation.toggle("/guide");
      expect(navigation.collapsed.has("/guide")).toBe(false);

      navigation.toggle("/guide");
      expect(navigation.collapsed.has("/guide")).toBe(true);
    });
  });

  describe("collapsed reactivity", () => {
    // `$state` does not proxy collections, so a plain Set would report this
    // mutation to nobody and the nav would silently stop re-rendering.
    it("reports an in-place add to a reader", () => {
      const navigation = new Navigation(mockApiClient);
      let seen: boolean | undefined;
      teardown = $effect.root(() => {
        const isCollapsed = $derived(navigation.collapsed.has("/guide"));
        $effect(() => {
          seen = isCollapsed;
        });
      });
      flushSync();
      expect(seen).toBe(false);

      navigation.collapsed.add("/guide");
      flushSync();
      expect(seen).toBe(true);

      navigation.collapsed.delete("/guide");
      flushSync();
      expect(seen).toBe(false);
    });
  });

  describe("expandOnlyTo", () => {
    it("expands every ancestor of the target", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      navigation.expandOnlyTo("/guide/advanced/plugins");

      expect(navigation.collapsed.has("/guide")).toBe(false);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);
    });

    it("collapses other sections only when the target is not already visible", async () => {
      mockFetchNavigation.mockResolvedValue(mockTree);
      const navigation = new Navigation(mockApiClient);
      await navigation.load();

      navigation.expandOnlyTo("/guide/getting-started");
      navigation.toggle("/guide/advanced");
      navigation.expandOnlyTo("/guide/getting-started");
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);

      navigation.toggle("/guide");
      navigation.expandOnlyTo("/guide/getting-started");
      expect(navigation.collapsed.has("/guide")).toBe(false);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(true);
    });

    it("expands to a non-ASCII page opened by URL", async () => {
      mockFetchNavigation.mockResolvedValue(
        scopedTree(rootSection, "/", [
          { title: "Гайд", path: "/гайд", children: [{ title: "Шаг", path: "/гайд/шаг" }] },
          { title: "Guide", path: "/guide", children: [{ title: "Intro", path: "/guide/intro" }] },
        ]),
      );
      const navigation = new Navigation(mockApiClient);
      await navigation.load();
      window.history.replaceState(null, "", "/гайд/шаг");

      navigation.expandOnlyTo(new Router().path);

      expect(navigation.collapsed.has("/гайд")).toBe(false);
      expect(navigation.collapsed.has("/guide")).toBe(true);
    });

    it("re-expands active path after load replaces the tree", async () => {
      mockFetchNavigation.mockResolvedValue(
        scopedTree(rootSection, "/", [
          {
            title: "Billing",
            path: "/billing",
            children: [
              { title: "Payments", path: "/billing/payments" },
              { title: "Invoices", path: "/billing/invoices" },
            ],
          },
        ]),
      );
      const navigation = new Navigation(mockApiClient);

      // Set active path before tree is loaded (simulates Page.svelte subscription)
      navigation.expandOnlyTo("/billing/payments");

      // Load navigation (simulates Page calling load)
      await navigation.load(ROOT_REF);

      // /billing should be expanded (ancestor of active path)
      expect(navigation.collapsed.has("/billing")).toBe(false);
    });

    it("re-expands active path when scope changes after initial load", async () => {
      const guideTree = scopedTree(guideSection, "/guide", [
        { title: "Getting Started", path: "/guide/getting-started" },
        {
          title: "Advanced",
          path: "/guide/advanced",
          children: [{ title: "Plugins", path: "/guide/advanced/plugins" }],
        },
      ]);

      mockFetchNavigation.mockResolvedValueOnce(mockTree).mockResolvedValueOnce(guideTree);
      const navigation = new Navigation(mockApiClient);

      // Initial load and expand (simulates Layout.svelte)
      await navigation.load();
      navigation.expandOnlyTo("/guide/advanced/plugins");

      // Verify expanded
      expect(navigation.collapsed.has("/guide")).toBe(false);
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);

      // Section change replaces the tree (simulates Page)
      await navigation.load(sectionRefString(guideSection));

      // Active path should still be expanded in the new tree
      expect(navigation.collapsed.has("/guide/advanced")).toBe(false);
    });
  });
});

describe("Navigation.isEmpty", () => {
  const billingTree = scopedTree(billingSection, "/billing", []);

  it.each<{ name: string; tree: NavigationTree | null; expected: boolean }>([
    { name: "before the tree loads", tree: null, expected: false },
    {
      name: "with no items and no parentScope",
      tree: scopedTree(rootSection, "/", []),
      expected: true,
    },
    {
      name: "with items",
      tree: scopedTree(rootSection, "/", [{ title: "Guide", path: "/guide" }]),
      expected: false,
    },
    {
      name: "for a childless section with a back-link",
      tree: { ...billingTree, parentScope: { path: "/", title: "Home", section: rootSection } },
      expected: false,
    },
  ])("is $expected $name", ({ tree, expected }) => {
    const nav = new Navigation(createMockApiClient());
    nav.tree = tree;
    expect(nav.isEmpty).toBe(expected);
  });
});
