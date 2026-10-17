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

describe("App init after unmount", () => {
  it("does not open a live-reload socket when config arrives after unmount", async () => {
    const { fetchFn, navigation, config } = controlledFetch();
    const { unmount } = render(App, { fetchFn: fetchFn as typeof fetch });

    navigation.resolve(json({ items: [] }));
    await vi.waitFor(() => expect(fetchFn).toHaveBeenCalledWith("/_api/config"));

    unmount();
    config.resolve(json({ liveReloadEnabled: true, commentsEnabled: true }));
    await flush();

    expect(webSocket).not.toHaveBeenCalled();
  });

  it("neither fetches config nor replays queued navigation after unmount", async () => {
    const { fetchFn, navigation } = controlledFetch();
    const onNavigate = vi.fn();
    let goto!: (path: string) => void;
    const { unmount } = render(App, {
      fetchFn: fetchFn as typeof fetch,
      embedded: true,
      initialPath: "/",
      sectionRef: "system:default/a",
      onNavigate,
      exposeGoto: (g: (path: string) => void) => {
        goto = g;
      },
    });
    goto("/guide");

    unmount();
    navigation.resolve(json({ items: [] }));
    await flush();

    expect(onNavigate).not.toHaveBeenCalled();
    expect(fetchFn).not.toHaveBeenCalledWith("/_api/config");
  });
});
