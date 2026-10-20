import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createApiClient, NotFoundError } from "./client";
import type { ApiClient } from "./client";
import type { ConfigResponse, NavigationTree, PageResponse } from "../types";

const mockNavTree: NavigationTree = {
  items: [{ title: "Guide", path: "/guide" }],
  scope: {
    path: "/",
    title: "Home",
    section: { kind: "section", namespace: "default", name: "root" },
  },
  sectionAncestry: {
    "section:default/root": [{ sectionRef: "section:default/root", subpath: "" }],
  },
};

const mockPage: PageResponse = {
  meta: {
    title: "Test",
    path: "/test",
    sectionRef: "section:default/root",
    subpath: "test",
    documentId: "section:default/root#test",
  },
  breadcrumbs: [],
  toc: [],
  content: "<p>Test</p>",
  sectionAncestry: {
    "section:default/root": [{ sectionRef: "section:default/root", subpath: "" }],
  },
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
    const client = createApiClient("/_api");
    const result = await client.fetchNavigation();

    expect(fetch).toHaveBeenCalledWith("/_api/navigation", {});
    expect(result).toEqual(mockNavTree);
  });

  it("passes cache: no-store when bypassCache is true", async () => {
    const client = createApiClient("/_api");
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
    const client = createApiClient("/_api");
    const result = await client.fetchPage("/test");

    expect(fetch).toHaveBeenCalledWith("/_api/pages/test", {});
    expect(result).toEqual(mockPage);
  });

  it.each([
    ["/", "/_api/pages/"],
    ["/гайд/шаг", "/_api/pages/%D0%B3%D0%B0%D0%B9%D0%B4/%D1%88%D0%B0%D0%B3"],
    ["/100%/a?b#c", "/_api/pages/100%25/a%3Fb%23c"],
  ])("encodes the decoded path %j", async (path, url) => {
    await createApiClient("/_api").fetchPage(path);

    expect(fetch).toHaveBeenCalledWith(url, {});
  });

  const signal = new AbortController().signal;

  it.each([
    { opts: { bypassCache: true }, init: { cache: "no-store" } },
    { opts: { signal }, init: { signal } },
    { opts: { bypassCache: true, signal }, init: { cache: "no-store", signal } },
  ])("maps $opts to fetch init $init", async ({ opts, init }) => {
    await createApiClient("/_api").fetchPage("/test", opts);

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

    const client = createApiClient("/_api");
    await expect(client.fetchPage("/missing")).rejects.toThrow(NotFoundError);
    await expect(client.fetchPage("/missing")).rejects.toThrow("Page not found: /missing");
  });
});

describe("NotFoundError", () => {
  it("has correct name and message", () => {
    const error = new NotFoundError("/missing/path");

    expect(error.name).toBe("NotFoundError");
    expect(error.message).toBe("Page not found: /missing/path");
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
    const client = createApiClient("/_api");
    const result = await client.fetchConfig();

    expect(fetch).toHaveBeenCalledWith("/_api/config");
    expect(result).toEqual(mockConfig);
  });
});

describe("createApiClient with custom base", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve({ ok: true, json: () => Promise.resolve({}) })),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it.each([
    { base: "/api/rw/", call: (c: ApiClient) => c.fetchConfig(), url: "/api/rw/config" },
    { base: "/api/rw", call: (c: ApiClient) => c.fetchConfig(), url: "/api/rw/config" },
    { base: "/api/rw", call: (c: ApiClient) => c.fetchPage("/guide"), url: "/api/rw/pages/guide" },
    {
      base: "/api/rw",
      call: (c: ApiClient) => c.fetchNavigation({ sectionRef: "domain:default/billing" }),
      url: "/api/rw/navigation?sectionRef=domain%3Adefault%2Fbilling",
    },
  ])("base $base requests $url", async ({ base, call, url }) => {
    await call(createApiClient(base));
    expect(vi.mocked(fetch).mock.calls[0]?.[0]).toBe(url);
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
    ["page", (c: ApiClient) => c.fetchPage("/test")],
    ["config", (c: ApiClient) => c.fetchConfig()],
  ] as const)("fetching %s throws with the status", async (what, call) => {
    await expect(call(createApiClient("/_api"))).rejects.toThrow(
      `Failed to fetch ${what}: 500 Internal Server Error`,
    );
  });
});
