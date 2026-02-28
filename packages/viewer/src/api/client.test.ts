import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createApiClient, NotFoundError } from "./client";
import type { ConfigResponse, NavigationTree, PageResponse } from "../types";

const mockNavTree: NavigationTree = {
  items: [{ title: "Home", path: "/" }],
};

const mockPage: PageResponse = {
  meta: {
    title: "Test",
    path: "/test",
    sourceFile: "test.md",
    lastModified: "2025-01-01T00:00:00Z",
    navigationScope: "",
  },
  breadcrumbs: [],
  toc: [],
  content: "<p>Test</p>",
};

describe("fetchNavigation", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockNavTree),
        }),
      ),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("fetches navigation from API", async () => {
    const client = createApiClient();
    const result = await client.fetchNavigation();

    expect(fetch).toHaveBeenCalledWith("/api/navigation", {});
    expect(result).toEqual(mockNavTree);
  });

  it("passes cache: no-store when bypassCache is true", async () => {
    const client = createApiClient();
    await client.fetchNavigation({ bypassCache: true });

    expect(fetch).toHaveBeenCalledWith("/api/navigation", { cache: "no-store" });
  });

  it("throws error on non-ok response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: false,
          status: 500,
          statusText: "Internal Server Error",
        }),
      ),
    );

    const client = createApiClient();
    await expect(client.fetchNavigation()).rejects.toThrow(
      "Failed to fetch navigation: 500 Internal Server Error",
    );
  });
});

describe("fetchPage", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockPage),
        }),
      ),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("fetches page from API", async () => {
    const client = createApiClient();
    const result = await client.fetchPage("test");

    expect(fetch).toHaveBeenCalledWith("/api/pages/test", {});
    expect(result).toEqual(mockPage);
  });

  it("passes cache: no-store when bypassCache is true", async () => {
    const client = createApiClient();
    await client.fetchPage("test", { bypassCache: true });

    expect(fetch).toHaveBeenCalledWith("/api/pages/test", { cache: "no-store" });
  });

  it("passes signal when provided", async () => {
    const controller = new AbortController();
    const client = createApiClient();
    await client.fetchPage("test", { signal: controller.signal });

    expect(fetch).toHaveBeenCalledWith("/api/pages/test", { signal: controller.signal });
  });

  it("passes both cache and signal when provided", async () => {
    const controller = new AbortController();
    const client = createApiClient();
    await client.fetchPage("test", { bypassCache: true, signal: controller.signal });

    expect(fetch).toHaveBeenCalledWith("/api/pages/test", {
      cache: "no-store",
      signal: controller.signal,
    });
  });

  it("throws NotFoundError on 404 response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: false,
          status: 404,
          statusText: "Not Found",
        }),
      ),
    );

    const client = createApiClient();
    await expect(client.fetchPage("missing")).rejects.toThrow(NotFoundError);
    await expect(client.fetchPage("missing")).rejects.toThrow("Page not found: missing");
  });

  it("throws generic error on other non-ok responses", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: false,
          status: 500,
          statusText: "Internal Server Error",
        }),
      ),
    );

    const client = createApiClient();
    await expect(client.fetchPage("test")).rejects.toThrow(
      "Failed to fetch page: 500 Internal Server Error",
    );
  });
});

describe("NotFoundError", () => {
  it("has correct name and message", () => {
    const error = new NotFoundError("/missing/path");

    expect(error.name).toBe("NotFoundError");
    expect(error.message).toBe("Page not found: /missing/path");
    expect(error.path).toBe("/missing/path");
  });

  it("is instance of Error", () => {
    const error = new NotFoundError("/test");

    expect(error).toBeInstanceOf(Error);
    expect(error).toBeInstanceOf(NotFoundError);
  });
});

const mockConfig: ConfigResponse = {
  liveReloadEnabled: true,
};

describe("fetchConfig", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockConfig),
        }),
      ),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("fetches config from API", async () => {
    const client = createApiClient();
    const result = await client.fetchConfig();

    expect(fetch).toHaveBeenCalledWith("/api/config");
    expect(result).toEqual(mockConfig);
  });

  it("throws error on non-ok response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: false,
          status: 500,
          statusText: "Internal Server Error",
        }),
      ),
    );

    const client = createApiClient();
    await expect(client.fetchConfig()).rejects.toThrow(
      "Failed to fetch config: 500 Internal Server Error",
    );
  });
});

describe("createApiClient with custom base", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ liveReloadEnabled: false }),
        }),
      ),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("strips trailing slash from base URL", async () => {
    const client = createApiClient("/api/rw/");
    await client.fetchConfig();
    expect(fetch).toHaveBeenCalledWith("/api/rw/config");
  });

  it("uses custom base URL for fetchConfig", async () => {
    const client = createApiClient("/api/rw");
    await client.fetchConfig();
    expect(fetch).toHaveBeenCalledWith("/api/rw/config");
  });

  it("uses custom base URL for fetchPage", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () =>
            Promise.resolve({
              meta: { title: "Test" },
              breadcrumbs: [],
              toc: [],
              content: "<p>Test</p>",
            }),
        }),
      ),
    );

    const client = createApiClient("/api/rw");
    await client.fetchPage("guide");
    expect(fetch).toHaveBeenCalledWith("/api/rw/pages/guide", expect.anything());
  });

  it("uses custom base URL for fetchNavigation with scope", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ items: [] }),
        }),
      ),
    );

    const client = createApiClient("/api/rw");
    await client.fetchNavigation({ scope: "domains" });
    expect(fetch).toHaveBeenCalledWith("/api/rw/navigation?scope=domains", expect.anything());
  });
});
