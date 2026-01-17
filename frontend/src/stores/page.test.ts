import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";
import type { PageResponse } from "../types";

// Mock the API client
vi.mock("../api/client", () => ({
  fetchPage: vi.fn(),
  NotFoundError: class NotFoundError extends Error {
    constructor(public path: string) {
      super(`Page not found: ${path}`);
      this.name = "NotFoundError";
    }
  },
}));

import { page } from "./page";
import { fetchPage, NotFoundError } from "../api/client";

const mockFetchPage = vi.mocked(fetchPage);

const mockPageResponse: PageResponse = {
  meta: {
    title: "Test Page",
    path: "/test",
    source_file: "test.md",
    last_modified: "2025-01-01T00:00:00Z",
  },
  breadcrumbs: [{ title: "Home", path: "/" }],
  toc: [{ level: 2, title: "Section", id: "section" }],
  content: "<h1>Test Page</h1><p>Content</p>",
};

describe("page store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    page.clear();
  });

  describe("load", () => {
    it("fetches page and updates state", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);

      await page.load("test");

      const state = get(page);
      expect(state.data).toEqual(mockPageResponse);
      expect(state.loading).toBe(false);
      expect(state.error).toBeNull();
      expect(state.notFound).toBe(false);
    });

    it("sets loading state while fetching", async () => {
      let resolvePromise: (value: PageResponse) => void;
      mockFetchPage.mockReturnValue(
        new Promise((resolve) => {
          resolvePromise = resolve;
        }),
      );

      const loadPromise = page.load("test");

      // Check loading state before resolve
      expect(get(page).loading).toBe(true);

      // Resolve and wait
      resolvePromise!(mockPageResponse);
      await loadPromise;

      expect(get(page).loading).toBe(false);
    });

    it("updates document title on successful load", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);

      await page.load("test");

      expect(document.title).toBe("Test Page - Docstage");
    });

    it("sets notFound on 404 error", async () => {
      mockFetchPage.mockRejectedValue(new NotFoundError("missing"));

      await page.load("missing");

      const state = get(page);
      expect(state.data).toBeNull();
      expect(state.loading).toBe(false);
      expect(state.error).toBeNull();
      expect(state.notFound).toBe(true);
    });

    it("sets error on other failures", async () => {
      mockFetchPage.mockRejectedValue(new Error("Server error"));

      await page.load("test");

      const state = get(page);
      expect(state.data).toBeNull();
      expect(state.loading).toBe(false);
      expect(state.error).toBe("Server error");
      expect(state.notFound).toBe(false);
    });

    it("handles non-Error exceptions", async () => {
      mockFetchPage.mockRejectedValue("String error");

      await page.load("test");

      const state = get(page);
      expect(state.error).toBe("Unknown error");
    });

    it("passes bypassCache option to fetch", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);

      await page.load("test", { bypassCache: true });

      expect(mockFetchPage).toHaveBeenCalledWith(
        "test",
        expect.objectContaining({ bypassCache: true }),
      );
    });

    it("passes AbortSignal to fetch", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);

      await page.load("test");

      expect(mockFetchPage).toHaveBeenCalledWith(
        "test",
        expect.objectContaining({ signal: expect.any(AbortSignal) }),
      );
    });

    it("clears previous error on new load", async () => {
      // First load with error
      mockFetchPage.mockRejectedValue(new Error("First error"));
      await page.load("test");
      expect(get(page).error).toBe("First error");

      // Second load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("test");
      expect(get(page).error).toBeNull();
    });

    it("clears previous notFound on new load", async () => {
      // First load returns 404
      mockFetchPage.mockRejectedValue(new NotFoundError("missing"));
      await page.load("missing");
      expect(get(page).notFound).toBe(true);

      // Second load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("test");
      expect(get(page).notFound).toBe(false);
    });

    it("ignores AbortError when request is cancelled", async () => {
      const abortError = new DOMException("Aborted", "AbortError");
      mockFetchPage.mockRejectedValue(abortError);

      await page.load("test");

      // State should remain in loading since AbortError is ignored
      // and no set() is called after the error
      const state = get(page);
      expect(state.loading).toBe(true);
      expect(state.error).toBeNull();
    });

    it("cancels previous request when new load is called", async () => {
      let capturedSignal: AbortSignal | undefined;
      mockFetchPage.mockImplementation((_path, options) => {
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

    it("resets state to loading when load is called", async () => {
      // First load succeeds
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("first");
      expect(get(page).data).not.toBeNull();

      // Set up a slow second load
      mockFetchPage.mockReturnValue(new Promise(() => {}));
      page.load("second");

      // State should be reset with data cleared
      const state = get(page);
      expect(state.data).toBeNull();
      expect(state.loading).toBe(true);
    });
  });

  describe("clear", () => {
    it("resets state to initial values", async () => {
      mockFetchPage.mockResolvedValue(mockPageResponse);
      await page.load("test");

      page.clear();

      const state = get(page);
      expect(state.data).toBeNull();
      expect(state.loading).toBe(false);
      expect(state.error).toBeNull();
      expect(state.notFound).toBe(false);
    });
  });
});
