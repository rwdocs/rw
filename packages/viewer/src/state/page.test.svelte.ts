import { describe, it, expect, vi, beforeEach } from "vitest";
import type { PageResponse } from "../types";
import { Page } from "./page.svelte";
import type { ApiClient } from "../api/client";
import { NotFoundError } from "../api/client";

const mockPageResponse: PageResponse = {
  meta: {
    title: "Test Page",
    path: "/test",
    sourceFile: "test.md",
    lastModified: "2025-01-01T00:00:00Z",
    navigationScope: "",
  },
  breadcrumbs: [{ title: "Home", path: "/" }],
  toc: [{ level: 2, title: "Section", id: "section" }],
  content: "<h1>Test Page</h1><p>Content</p>",
};

function createMockApiClient(overrides: Record<string, ReturnType<typeof vi.fn>> = {}): ApiClient {
  return {
    fetchConfig: vi.fn(),
    fetchPage: vi.fn(),
    fetchNavigation: vi.fn(),
    ...overrides,
  } as unknown as ApiClient;
}

describe("page store", () => {
  let mockApiClient: ApiClient;
  let mockFetchPage: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    mockFetchPage = vi.fn();
    mockApiClient = createMockApiClient({ fetchPage: mockFetchPage });
  });

  describe("load", () => {
    it("fetches page and updates state", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("test");

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

      const loadPromise = page.load("test");

      // Check loading state before resolve
      expect(page.loading).toBe(true);

      // Resolve and wait
      resolvePromise!(mockPageResponse);
      await loadPromise;

      expect(page.loading).toBe(false);
    });

    it("updates document title on successful load", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("test");

      expect(document.title).toBe("Test Page - RW");
    });

    it("does not update document title in embedded mode", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      document.title = "Host App";
      const page = new Page(mockApiClient, { embedded: true });

      await page.load("test");

      expect(document.title).toBe("Host App");
    });

    it("sets notFound on 404 error", async () => {
      mockFetchPage.mockRejectedValue(new NotFoundError("missing"));
      const page = new Page(mockApiClient);

      await page.load("missing");

      expect(page.data).toBeNull();
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(true);
    });

    it("sets error on other failures", async () => {
      mockFetchPage.mockRejectedValue(new Error("Server error"));
      const page = new Page(mockApiClient);

      await page.load("test");

      expect(page.data).toBeNull();
      expect(page.loading).toBe(false);
      expect(page.error).toBe("Server error");
      expect(page.notFound).toBe(false);
    });

    it("handles non-Error exceptions", async () => {
      mockFetchPage.mockRejectedValue("String error");
      const page = new Page(mockApiClient);

      await page.load("test");

      expect(page.error).toBe("Unknown error");
    });

    it("passes bypassCache option to fetch", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("test", { bypassCache: true });

      expect(mockFetchPage).toHaveBeenCalledWith(
        "test",
        expect.objectContaining({ bypassCache: true }),
      );
    });

    it("passes AbortSignal to fetch", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);

      await page.load("test");

      expect(mockFetchPage).toHaveBeenCalledWith(
        "test",
        expect.objectContaining({ signal: expect.any(AbortSignal) }),
      );
    });

    it("clears previous error on new load", async () => {
      const page = new Page(mockApiClient);

      // First load with error
      mockFetchPage.mockRejectedValue(new Error("First error"));
      await page.load("test");
      expect(page.error).toBe("First error");

      // Second load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("test");
      expect(page.error).toBeNull();
    });

    it("clears previous notFound on new load", async () => {
      const page = new Page(mockApiClient);

      // First load returns 404
      mockFetchPage.mockRejectedValue(new NotFoundError("missing"));
      await page.load("missing");
      expect(page.notFound).toBe(true);

      // Second load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("test");
      expect(page.notFound).toBe(false);
    });

    it("ignores AbortError when request is cancelled", async () => {
      const page = new Page(mockApiClient);
      const abortError = new DOMException("Aborted", "AbortError");
      mockFetchPage.mockRejectedValue(abortError);

      await page.load("test");

      // State should remain in loading since AbortError is ignored
      // and no set() is called after the error
      expect(page.loading).toBe(true);
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
      const firstLoad = page.load("first");

      // Capture the signal from first request
      const firstSignal = capturedSignal;
      expect(firstSignal?.aborted).toBe(false);

      // Start second load immediately
      page.load("second");

      // First signal should now be aborted
      expect(firstSignal?.aborted).toBe(true);

      // Wait for loads to complete
      await firstLoad;
    });

    it("preserves previous data during new load for smooth transitions", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("first");
      expect(page.data).not.toBeNull();

      // Set up a slow second load
      mockFetchPage.mockReturnValue(new Promise(() => {}));
      page.load("second");

      // Previous data should be preserved for smooth transitions, loading should be true
      expect(page.data).not.toBeNull();
      expect(page.loading).toBe(true);
    });

    it("skips loading state when silent option is true", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("first");

      // Set up a slow second load with silent option
      let resolvePromise: (value: PageResponse) => void;
      mockFetchPage.mockReturnValue(
        new Promise((resolve) => {
          resolvePromise = resolve;
        }),
      );

      const loadPromise = page.load("second", { silent: true });

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

    it("preserves existing data during silent load", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("first");
      const originalData = page.data;

      // Start silent load that doesn't resolve
      mockFetchPage.mockReturnValue(new Promise(() => {}));
      page.load("second", { silent: true });

      // Original data should still be present
      expect(page.data).toBe(originalData);
    });

    it("updates data after silent load completes", async () => {
      const page = new Page(mockApiClient);

      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("first");

      // Second load with silent option
      const updatedResponse = { ...mockPageResponse, content: "<h1>New Content</h1>" };
      mockFetchPage.mockResolvedValue(updatedResponse);
      await page.load("second", { silent: true, bypassCache: true });

      // Data should be updated without going through loading state
      expect(page.data).toEqual(updatedResponse);
      expect(page.loading).toBe(false);
    });
  });

  describe("clear", () => {
    it("resets state to initial values", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      const page = new Page(mockApiClient);
      await page.load("test");

      page.clear();

      expect(page.data).toBeNull();
      expect(page.loading).toBe(false);
      expect(page.error).toBeNull();
      expect(page.notFound).toBe(false);
    });
  });
});
