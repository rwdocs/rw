import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Mock } from "vitest";
import type { Breadcrumb, PageResponse } from "../types";
import { Page } from "./page.svelte";
import type { ApiClient } from "../api/client";
import { NotFoundError } from "../api/client";
import { SectionLinks } from "$lib/sectionRefs";

const mockPageResponse: PageResponse = {
  meta: {
    title: "Test Page",
    path: "/test",
    sectionRef: "section:default/root",
    subpath: "test",
    documentId: "section:default/root#test",
  },
  breadcrumbs: [{ title: "Home", path: "/", sectionRef: "section:default/root", subpath: "" }],
  toc: [{ level: 2, title: "Section", id: "section" }],
  content: "<h1>Test Page</h1><p>Content</p>",
  sectionAncestry: {
    "section:default/root": [{ sectionRef: "section:default/root", subpath: "" }],
  },
};

const responseWithBreadcrumbs: PageResponse = {
  ...mockPageResponse,
  breadcrumbs: [{ title: "API", path: "/", sectionRef: "component:default/api", subpath: "" }],
  sectionAncestry: {
    "component:default/api": [{ sectionRef: "component:default/api", subpath: "" }],
  },
};

function createMockApiClient(overrides: Record<string, ReturnType<typeof vi.fn>> = {}): ApiClient {
  return {
    fetchConfig: vi.fn(),
    fetchPage: vi.fn(),
    fetchNavigation: vi.fn(),
    ...overrides,
  };
}

describe("page store", () => {
  let mockApiClient: ApiClient;
  let mockFetchPage: Mock<ApiClient["fetchPage"]>;

  beforeEach(() => {
    mockFetchPage = vi.fn();
    mockApiClient = createMockApiClient({ fetchPage: mockFetchPage });
  });

  describe("load", () => {
    it("fetches page and updates state", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("/test");

      expect(page.data).toEqual(mockPageResponse);
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
    });

    it("sets loading state while fetching", async () => {
      let resolvePromise: (value: PageResponse) => void;
      mockFetchPage.mockReturnValue(
        new Promise((resolve) => {
          resolvePromise = resolve;
        }),
      );
      const page = new Page(mockApiClient);

      const loadPromise = page.load("/test");

      // Check loading state before resolve
      expect(page.loading).toBe(true);

      // Resolve and wait
      resolvePromise!(mockPageResponse);
      await loadPromise;

      expect(page.loading).toBe(false);
    });

    it("sets notFound on 404 error", async () => {
      mockFetchPage.mockRejectedValue(new NotFoundError("missing"));
      const page = new Page(mockApiClient);

      await page.load("/missing");

      expect(page.data).toBeNull();
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(true);
    });

    it("sets error on other failures", async () => {
      mockFetchPage.mockRejectedValue(new Error("Server error"));
      const page = new Page(mockApiClient);

      await page.load("/test");

      expect(page.data).toBeNull();
      expect(page.loading).toBe(false);
      expect(page.error).toBe("Server error");
      expect(page.notFound).toBe(false);
    });

    it("handles non-Error exceptions", async () => {
      mockFetchPage.mockRejectedValue("String error");
      const page = new Page(mockApiClient);

      await page.load("/test");

      expect(page.error).toBe("Unknown error");
    });

    it("passes bypassCache option to fetch", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("/test", { bypassCache: true });

      expect(mockFetchPage).toHaveBeenCalledWith(
        "/test",
        expect.objectContaining({ bypassCache: true }),
      );
    });

    it("passes AbortSignal to fetch", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("/test");

      expect(mockFetchPage).toHaveBeenCalledWith(
        "/test",
        // `expect.any` is typed `any` — it returns an asymmetric matcher, not
        // an AbortSignal, so there is no honest annotation for it.
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
        expect.objectContaining({ signal: expect.any(AbortSignal) }),
      );
    });

    it("clears previous error on new load", async () => {
      const page = new Page(mockApiClient);

      // First load with error
      mockFetchPage.mockRejectedValue(new Error("First error"));
      await page.load("/test");
      expect(page.error).toBe("First error");

      // Second load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/test");
      expect(page.error).toBeNull();
    });

    it("clears previous notFound on new load", async () => {
      const page = new Page(mockApiClient);

      // First load returns 404
      mockFetchPage.mockRejectedValue(new NotFoundError("missing"));
      await page.load("/missing");
      expect(page.notFound).toBe(true);

      // Second load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/test");
      expect(page.notFound).toBe(false);
    });

    it("surfaces an AbortError raised by the host fetch itself (e.g. its own timeout)", async () => {
      const page = new Page(mockApiClient);
      mockFetchPage.mockRejectedValue(new DOMException("Aborted", "AbortError"));

      await page.load("/test");

      expect(page.loading).toBe(false);
      // Not the message: jsdom's DOMException, unlike a browser's, is no Error.
      expect(page.error).not.toBeNull();
    });

    it("ignores the AbortError of a load it superseded", async () => {
      const page = new Page(mockApiClient);
      mockFetchPage.mockImplementationOnce(
        (_path: string, options?: { signal?: AbortSignal }) =>
          new Promise<PageResponse>((_, reject) => {
            options?.signal?.addEventListener("abort", () =>
              reject(new DOMException("Aborted", "AbortError")),
            );
          }),
      );
      const firstLoad = page.load("/first");

      mockFetchPage.mockResolvedValueOnce(mockPageResponse);
      await page.load("/second");
      await firstLoad;

      expect(page.data).toEqual(mockPageResponse);
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
    });

    it("cancels previous request when new load is called", async () => {
      const page = new Page(mockApiClient);
      let capturedSignal: AbortSignal | undefined;
      mockFetchPage.mockImplementation((_path: string, options?: { signal?: AbortSignal }) => {
        capturedSignal = options?.signal;
        return new Promise((resolve) => setTimeout(() => resolve(mockPageResponse), 100));
      });

      // Start first load
      const firstLoad = page.load("/first");

      // Capture the signal from first request
      const firstSignal = capturedSignal;
      expect(firstSignal?.aborted).toBe(false);

      // Start second load immediately
      void page.load("/second");

      // First signal should now be aborted
      expect(firstSignal?.aborted).toBe(true);

      // Wait for loads to complete
      await firstLoad;
    });

    it("ignores a superseded non-abort rejection (does not clobber fresh state)", async () => {
      const page = new Page(mockApiClient);

      // First load hangs until we explicitly reject it.
      let rejectFirst: (e: Error) => void;
      const firstPromise = new Promise<PageResponse>((_, reject) => {
        rejectFirst = reject;
      });
      mockFetchPage.mockReturnValueOnce(firstPromise);
      const firstLoad = page.load("/first");

      // Second load supersedes the first (aborting it) and resolves successfully.
      mockFetchPage.mockResolvedValueOnce(mockPageResponse);
      await page.load("/second");
      expect(page.data).toEqual(mockPageResponse);
      expect(page.error).toBeNull();

      // The superseded first load now rejects with a PLAIN Error (not AbortError).
      rejectFirst!(new Error("network blip"));
      await firstLoad;

      // The winning load's state must be intact.
      expect(page.data).toEqual(mockPageResponse);
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
    });

    it("preserves previous data during new load for smooth transitions", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/first");
      expect(page.data).not.toBeNull();

      // Set up a slow second load
      mockFetchPage.mockReturnValue(new Promise(() => {}));
      void page.load("/second");

      // Previous data should be preserved for smooth transitions, loading should be true
      expect(page.data).not.toBeNull();
      expect(page.loading).toBe(true);
    });

    it("skips loading state when silent option is true", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/first");

      // Set up a slow second load with silent option
      let resolvePromise: (value: PageResponse) => void;
      mockFetchPage.mockReturnValue(
        new Promise((resolve) => {
          resolvePromise = resolve;
        }),
      );

      const loadPromise = page.load("/second", { silent: true });

      // Loading should still be false, data should be preserved
      expect(page.loading).toBe(false);
      expect(page.data).toEqual(mockPageResponse);

      // Complete the load
      const newResponse = { ...mockPageResponse, content: "<h1>Updated</h1>" };
      resolvePromise!(newResponse);
      await loadPromise;

      // Data should be updated
      expect(page.data).toEqual(newResponse);
    });

    it("keeps the shown data when a silent load returns the same page", async () => {
      const page = new Page(mockApiClient);
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/test");
      const shown = page.data;

      mockFetchPage.mockResolvedValue(structuredClone(mockPageResponse));
      await page.load("/test", { silent: true });

      expect(page.data).toBe(shown);
      expect(page.loading).toBe(false);
    });

    it("preserves data and stays error-free when a silent load rejects (generic)", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds.
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/first");
      const originalData = page.data;

      // Silent reload rejects with a transient error.
      mockFetchPage.mockRejectedValueOnce(new Error("network down"));
      await page.load("/first", { silent: true });

      // Last-known-good content is kept; no visible error/notFound.
      expect(page.data).toBe(originalData);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
      expect(page.loading).toBe(false);
    });

    it("clears the spinner when a silent load supersedes an in-flight non-silent load and then fails", async () => {
      const page = new Page(mockApiClient);

      // Seed last-known-good content.
      mockFetchPage.mockResolvedValueOnce(mockPageResponse);
      await page.load("/first");
      const originalData = page.data;

      // A non-silent navigation starts and hangs → loading=true.
      mockFetchPage.mockReturnValueOnce(new Promise<PageResponse>(() => {}));
      void page.load("/second"); // not awaited — stays pending
      expect(page.loading).toBe(true);

      // A live-reload silent refresh supersedes it (aborting it), then fails.
      mockFetchPage.mockRejectedValueOnce(new Error("server restarting"));
      await page.load("/second", { silent: true });

      // Spinner cleared, last-known-good kept, no visible error.
      expect(page.loading).toBe(false);
      expect(page.data).toBe(originalData);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
    });

    it("preserves data on a silent 404 (uniform behavior)", async () => {
      const page = new Page(mockApiClient);

      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("/first");
      const originalData = page.data;

      // Silent reload now 404s — uniform: keep the stale page, do not flip notFound.
      mockFetchPage.mockRejectedValueOnce(new NotFoundError("missing"));
      await page.load("/first", { silent: true });

      expect(page.data).toBe(originalData);
      expect(page.notFound).toBe(false);
      expect(page.error).toBeNull();
    });

    it("drops a load superseded while its breadcrumbs were resolving", async () => {
      let resolveFirst!: (v: Record<string, string>) => void;
      const resolver = vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<Record<string, string>>((r) => {
              resolveFirst = r;
            }),
        )
        .mockResolvedValue({ "component:default/api": "/docs/api" });
      const page = new Page(mockApiClient, new SectionLinks(resolver));

      mockFetchPage.mockResolvedValueOnce(responseWithBreadcrumbs);
      const first = page.load("/first");
      await vi.waitFor(() => expect(resolver).toHaveBeenCalledTimes(1));

      mockFetchPage.mockResolvedValueOnce({ ...responseWithBreadcrumbs, content: "<p>second</p>" });
      await page.load("/second");

      resolveFirst({ "component:default/api": "/docs/api" });
      await first;

      expect(page.data?.content).toBe("<p>second</p>");
      expect(page.error).toBeNull();
    });

    it("keeps the fetched page with unresolved breadcrumbs when the section-ref resolver rejects", async () => {
      mockFetchPage.mockResolvedValue(responseWithBreadcrumbs);
      const resolver = vi.fn().mockRejectedValue(new Error("host resolver failed"));
      const page = new Page(mockApiClient, new SectionLinks(resolver));

      await page.load("/test");

      // The fetch succeeded; resolver rejection must not blank the page or
      // surface an error/notFound — only breadcrumb rewriting is skipped.
      expect(page.data).toEqual(responseWithBreadcrumbs);
      expect(page.crumbHref(responseWithBreadcrumbs.breadcrumbs[0])).toBeUndefined();
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
      expect(page.loading).toBe(false);
    });

    it("resolves crumbs via their nearest mapped section, encoding the subpath", async () => {
      const crumbs: Breadcrumb[] = [
        { title: "API", path: "/api", sectionRef: "component:default/api", subpath: "" },
        {
          title: "Intro",
          path: "/api/c#/a?b",
          sectionRef: "component:default/api",
          subpath: "c#/a?b",
        },
        { title: "Home", path: "/", sectionRef: "section:default/root", subpath: "" },
      ];
      mockFetchPage.mockResolvedValue({
        ...mockPageResponse,
        breadcrumbs: crumbs,
        sectionAncestry: {
          "component:default/api": [
            { sectionRef: "component:default/api", subpath: "" },
            { sectionRef: "section:default/root", subpath: "api" },
          ],
          "section:default/root": [{ sectionRef: "section:default/root", subpath: "" }],
        },
      });
      const resolver = vi.fn().mockResolvedValue({ "section:default/root": "/host" });
      const page = new Page(mockApiClient, new SectionLinks(resolver));

      await page.load("/api/c#/a?b/page");

      expect(crumbs.map(page.crumbHref)).toEqual(["/host/api", "/host/api/c%23/a%3Fb", "/host"]);
    });

    it("asks the host about each section once across loads", async () => {
      mockFetchPage.mockResolvedValue(responseWithBreadcrumbs);
      const resolver = vi.fn().mockResolvedValue({ "component:default/api": "/docs/api" });
      const page = new Page(mockApiClient, new SectionLinks(resolver));

      await page.load("/first");
      await page.load("/second");

      expect(resolver).toHaveBeenCalledTimes(1);
      expect(page.crumbHref(responseWithBreadcrumbs.breadcrumbs[0])).toBe("/docs/api");
    });

    it("still errors when the FETCH itself rejects, even with a section-ref resolver configured", async () => {
      mockFetchPage.mockRejectedValue(new Error("Server error"));
      const resolver = vi.fn().mockResolvedValue({});
      const page = new Page(mockApiClient, new SectionLinks(resolver));

      await page.load("/test");

      expect(page.data).toBeNull();
      expect(page.error).toBe("Server error");
      expect(page.notFound).toBe(false);
      // The resolver is never reached because fetchPage rejected first.
      expect(resolver).not.toHaveBeenCalled();
    });
  });

  describe("clear", () => {
    it("resets state to initial values", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);
      await page.load("/test");

      page.clear();

      expect(page.data).toBeNull();
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
    });
  });
});
