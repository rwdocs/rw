import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import App from "./App.svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200 });
}

const webSocket = vi.fn();

beforeEach(() => {
  webSocket.mockClear();
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
  vi.stubGlobal("WebSocket", webSocket);
  vi.stubGlobal("scrollTo", vi.fn());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

/** A fetch whose `/navigation` and `/config` responses the test releases by hand;
 *  page fetches never settle. */
function controlledFetch() {
  const navigation = deferred<Response>();
  const config = deferred<Response>();
  const fetchFn = vi.fn((url: string) => {
    if (url.includes("/navigation")) return navigation.promise;
    if (url.endsWith("/config")) return config.promise;
    return new Promise<Response>(() => {});
  });
  return { fetchFn, navigation, config };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

const rootSection = { kind: "section", namespace: "default", name: "root" };
const billingSection = { kind: "domain", namespace: "default", name: "billing" };

function navTree(scope: typeof rootSection, path: string) {
  const ref = `${scope.kind}:${scope.namespace}/${scope.name}`;
  return {
    items: [],
    scope: { path, title: scope.name, section: scope },
    sectionAncestry: { [ref]: [{ sectionRef: ref, subpath: "" }] },
  };
}

function pageResponse(path: string, sectionRef: string) {
  return {
    meta: { title: "T", path: `/${path}`, sectionRef, subpath: path },
    breadcrumbs: [],
    toc: [],
    content: "<p>body</p>",
    sectionAncestry: { [sectionRef]: [{ sectionRef, subpath: "" }] },
  };
}

/** A fetch serving one page plus root and billing navigation; the test
 *  releases the first navigation and page responses by hand to pick which
 *  lands first. */
function startupFetch(pageSectionRef: string) {
  const firstNavigation = deferred<void>();
  const page = deferred<void>();
  const navigationUrls: string[] = [];
  const fetchFn = vi.fn(async (url: string) => {
    if (url.includes("/navigation")) {
      navigationUrls.push(url);
      if (navigationUrls.length === 1) await firstNavigation.promise;
      return url.includes("sectionRef=")
        ? json(navTree(billingSection, "/domains/billing"))
        : json(navTree(rootSection, "/"));
    }
    if (url.includes("/pages/")) {
      await page.promise;
      return json(pageResponse(url.split("/pages/")[1] ?? "", pageSectionRef));
    }
    return json({ liveReloadEnabled: false });
  });
  return { fetchFn, firstNavigation, page, navigationUrls };
}

function visit(path: string) {
  window.history.replaceState(null, "", path);
}

describe("App startup requests", () => {
  afterEach(() => visit("/"));

  it.each([
    ["root page, navigation first", "/", "navigation"],
    ["root page, page first", "/", "page"],
    ["nested page in the root section, page first", "/guide/start", "page"],
  ] as const)("fetches navigation once: %s", async (_, path, first) => {
    visit(path);
    const { fetchFn, firstNavigation, page, navigationUrls } = startupFetch("section:default/root");
    render(App, { fetchFn: fetchFn as typeof fetch });

    const [a, b] = first === "page" ? [page, firstNavigation] : [firstNavigation, page];
    a.resolve();
    await flush();
    b.resolve();
    await vi.waitFor(() => expect(fetchFn).toHaveBeenCalledWith("/_api/config"));
    await flush();
    await flush();

    expect(navigationUrls).toEqual(["/_api/navigation"]);
  });

  it("loads the page's own section when it differs from the root", async () => {
    visit("/domains/billing/guide");
    const { fetchFn, firstNavigation, page, navigationUrls } =
      startupFetch("domain:default/billing");
    render(App, { fetchFn: fetchFn as typeof fetch });

    firstNavigation.resolve();
    page.resolve();
    await vi.waitFor(() => expect(navigationUrls).toHaveLength(2));

    expect(navigationUrls[1]).toBe("/_api/navigation?sectionRef=domain%3Adefault%2Fbilling");
  });

  it("fetches config without waiting for navigation", async () => {
    const { fetchFn } = controlledFetch();
    render(App, { fetchFn: fetchFn as typeof fetch });

    await vi.waitFor(() => expect(fetchFn).toHaveBeenCalledWith("/_api/config"));
  });
});

describe("App goto", () => {
  it("takes a path relative to the viewer's base, and does not report it back", async () => {
    const { fetchFn, navigation } = controlledFetch();
    const onNavigate = vi.fn();
    const { component } = render(App, {
      fetchFn: fetchFn as typeof fetch,
      embedded: true,
      initialPath: "/",
      sectionRef: "domain:default/billing",
      onNavigate,
    });
    const app: { goto: (path: string) => void } = component;

    navigation.resolve(json(navTree(billingSection, "/domains/billing")));
    await vi.waitFor(() =>
      expect(fetchFn).toHaveBeenCalledWith("/_api/pages/domains/billing", expect.anything()),
    );
    app.goto("/api#intro");

    await vi.waitFor(() =>
      expect(fetchFn).toHaveBeenCalledWith("/_api/pages/domains/billing/api", expect.anything()),
    );
    expect(onNavigate).not.toHaveBeenCalled();
  });
});

describe("App init after unmount", () => {
  it("does not open a live-reload socket when config arrives after unmount", async () => {
    const { fetchFn, navigation, config } = controlledFetch();
    const { unmount } = render(App, { fetchFn: fetchFn as typeof fetch });

    navigation.resolve(json(navTree(rootSection, "/")));
    await vi.waitFor(() => expect(fetchFn).toHaveBeenCalledWith("/_api/config"));

    unmount();
    config.resolve(json({ liveReloadEnabled: true, commentsEnabled: true }));
    await flush();

    expect(webSocket).not.toHaveBeenCalled();
  });
});
