import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createApiClient, NotFoundError } from "./client";
import type { ApiClient } from "./client";
import type { ConfigResponse, NavigationTree, PageResponse } from "../types";

const mockNavTree: NavigationTree = {
  items: [{ title: "Home", path: "/" }],
};

const mockPage: PageResponse = {
  meta: {
    title: "Test",
    path: "/test",
    sectionRef: "section:default/root",
    subpath: "test",
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

    expect(fetch).toHaveBeenCalledWith("/_api/navigation", {});
    expect(result).toEqual(mockNavTree);
  });

  it("passes cache: no-store when bypassCache is true", async () => {
    const client = createApiClient();
    await client.fetchNavigation({ bypassCache: true });

    expect(fetch).toHaveBeenCalledWith("/_api/navigation", { cache: "no-store" });
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

    expect(fetch).toHaveBeenCalledWith("/_api/pages/test", {});
    expect(result).toEqual(mockPage);
  });

  const signal = new AbortController().signal;

  it.each([
    { opts: { bypassCache: true }, init: { cache: "no-store" } },
    { opts: { signal }, init: { signal } },
    { opts: { bypassCache: true, signal }, init: { cache: "no-store", signal } },
  ])("maps $opts to fetch init $init", async ({ opts, init }) => {
    await createApiClient().fetchPage("test", opts);

    expect(fetch).toHaveBeenCalledWith("/_api/pages/test", init);
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
});

describe("NotFoundError", () => {
  it("has correct name and message", () => {
    const error = new NotFoundError("/missing/path");

    expect(error.name).toBe("NotFoundError");
    expect(error.message).toBe("Page not found: /missing/path");
    expect(error.path).toBe("/missing/path");
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

    expect(fetch).toHaveBeenCalledWith("/_api/config");
    expect(result).toEqual(mockConfig);
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

  it("uses custom base URL for fetchNavigation with sectionRef", async () => {
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
    await client.fetchNavigation({ sectionRef: "domain:default/billing" });
    expect(fetch).toHaveBeenCalledWith(
      "/api/rw/navigation?sectionRef=domain%3Adefault%2Fbilling",
      expect.anything(),
    );
  });
});

describe("non-ok responses", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve({ ok: false, status: 500, statusText: "Internal Server Error" })),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it.each([
    ["navigation", (c: ApiClient) => c.fetchNavigation()],
    ["page", (c: ApiClient) => c.fetchPage("test")],
    ["config", (c: ApiClient) => c.fetchConfig()],
  ] as const)("fetching %s throws with the status", async (what, call) => {
    await expect(call(createApiClient())).rejects.toThrow(
      `Failed to fetch ${what}: 500 Internal Server Error`,
    );
  });
});
