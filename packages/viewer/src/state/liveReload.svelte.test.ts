import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { LiveReload } from "./liveReload.svelte";
import { Router } from "./router.svelte";

// Mock WebSocket
class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;

  constructor(public url: string) {
    mockWebSocketInstances.push(this);
  }

  // Like a browser: the close event is queued, and the handler is looked up
  // when it fires, not when close() is called.
  close() {
    if (this.readyState >= MockWebSocket.CLOSING) {
      return;
    }
    this.readyState = MockWebSocket.CLOSING;
    setTimeout(() => this.simulateClose(), 0);
  }

  // Test helpers
  simulateClose() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  simulateOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }

  simulateError() {
    this.onerror?.();
  }
}

let mockWebSocketInstances: MockWebSocket[] = [];

function createMockRouter(currentPath: string = "/docs/guide"): Router {
  return {
    path: currentPath,
    hash: "",
    embedded: false,
    prefixPath: (path: string) => path,
    goto: vi.fn(),
    initRouter: vi.fn(() => () => {}),
  } as unknown as Router;
}

describe("liveReload store", () => {
  let mockRouter: Router;

  beforeEach(() => {
    vi.useFakeTimers();
    mockWebSocketInstances = [];

    // Setup WebSocket mock
    vi.stubGlobal("WebSocket", MockWebSocket);

    // Setup location mock
    vi.stubGlobal("location", {
      protocol: "http:",
      host: "localhost:7979",
    });

    mockRouter = createMockRouter();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  function createStore() {
    return new LiveReload({ router: mockRouter });
  }

  function connected() {
    const liveReload = createStore();
    liveReload.connect();
    const ws = mockWebSocketInstances[0];
    ws.simulateOpen();
    return { liveReload, ws };
  }

  describe("connect()", () => {
    it("creates WebSocket connection", () => {
      const liveReload = createStore();
      liveReload.connect();

      expect(mockWebSocketInstances.length).toBe(1);
      expect(mockWebSocketInstances[0].url).toBe("ws://localhost:7979/ws/live-reload");
    });

    it("uses wss protocol for https", () => {
      vi.stubGlobal("location", {
        protocol: "https:",
        host: "localhost:7979",
      });

      const liveReload = createStore();
      liveReload.connect();

      const lastInstance = mockWebSocketInstances[mockWebSocketInstances.length - 1];
      expect(lastInstance.url).toBe("wss://localhost:7979/ws/live-reload");

      liveReload.disconnect();
    });

    it("does not create duplicate connection if already open", () => {
      const { liveReload } = connected();

      liveReload.connect();

      expect(mockWebSocketInstances.length).toBe(1);
    });

    it("does not create a second connection while the first is connecting", () => {
      const liveReload = createStore();
      liveReload.connect();

      liveReload.connect();

      expect(mockWebSocketInstances.length).toBe(1);
    });
  });

  describe("replaced socket", () => {
    function reconnected() {
      const liveReload = createStore();
      liveReload.connect();
      const old = mockWebSocketInstances[0];
      liveReload.disconnect();
      liveReload.connect();
      const current = mockWebSocketInstances[1];
      current.simulateOpen();
      return { liveReload, old, current };
    }

    it("a late error does not close the current socket", () => {
      const { old, current } = reconnected();

      old.simulateError();

      expect(current.readyState).toBe(MockWebSocket.OPEN);
    });

    it("a late message is ignored", () => {
      const { liveReload, old } = reconnected();
      const callback = vi.fn();
      liveReload.on("structure", callback);

      old.simulateMessage({ type: "structure" });

      expect(callback).not.toHaveBeenCalled();
    });

    it("a late close does not schedule a reconnect", () => {
      const { old } = reconnected();

      old.simulateClose();
      vi.advanceTimersByTime(5000);

      expect(mockWebSocketInstances.length).toBe(2);
    });
  });

  describe("connection events", () => {
    it("schedules reconnect on close", () => {
      const { ws } = connected();
      ws.simulateClose();

      expect(mockWebSocketInstances.length).toBe(1);

      vi.advanceTimersByTime(2000);

      expect(mockWebSocketInstances.length).toBe(2);
    });

    it("does not refetch the page on the first open", () => {
      const liveReload = createStore();
      const callback = vi.fn();
      liveReload.on("content", callback);

      liveReload.connect();
      mockWebSocketInstances[0].simulateOpen();

      expect(callback).not.toHaveBeenCalled();
    });

    it("refetches the page when the socket reopens after a drop", () => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      ws.simulateClose();
      vi.advanceTimersByTime(2000);
      mockWebSocketInstances[1].simulateClose();
      vi.advanceTimersByTime(2000);
      expect(callback).not.toHaveBeenCalled();
      mockWebSocketInstances[2].simulateOpen();

      expect(callback).toHaveBeenCalledTimes(1);
      // Images may have changed while it was down, too.
      expect(liveReload.filesVersion).toBe(1);
    });

    it("does not refetch the page on the first open after disconnect()", () => {
      const { liveReload } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      liveReload.disconnect();
      liveReload.connect();
      mockWebSocketInstances[1].simulateOpen();

      expect(callback).not.toHaveBeenCalled();
    });

    it("closes connection on error", () => {
      const { ws } = connected();

      ws.simulateError();

      expect(ws.readyState).toBe(MockWebSocket.CLOSING);
    });
  });

  describe("on()", () => {
    it.each(["structure", "comments"] as const)("calls the %s listener", (type) => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on(type, callback);

      ws.simulateMessage({ type });

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it("calls the content listener for the current page", () => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      ws.simulateMessage({ type: "content", path: "/docs/guide" });

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it("does not call the content listener for a different page", () => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      ws.simulateMessage({ type: "content", path: "/docs/other" });

      expect(callback).not.toHaveBeenCalled();
    });

    it("calls the content listener on any page when an included file or image changes", () => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      ws.simulateMessage({ type: "files" });

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it("bumps filesVersion when a file changes", () => {
      const { liveReload, ws } = connected();

      ws.simulateMessage({ type: "content", path: "/docs/guide" });
      expect(liveReload.filesVersion).toBe(0);
      ws.simulateMessage({ type: "files" });
      expect(liveReload.filesVersion).toBe(1);
    });

    it("matches a non-ASCII page, which the server names in raw UTF-8", () => {
      vi.stubGlobal("location", {
        protocol: "http:",
        host: "localhost:7979",
        pathname: "/%D0%B3%D0%B0%D0%B9%D0%B4",
        hash: "",
      });
      mockRouter = new Router();
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      ws.simulateMessage({ type: "content", path: "/гайд" });

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it("calls the content listener on any page when the site structure changes", () => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("content", callback);

      ws.simulateMessage({ type: "structure" });

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it("routes each message only to the listener of its type", () => {
      const { liveReload, ws } = connected();
      const content = vi.fn();
      const structure = vi.fn();
      liveReload.on("content", content);
      liveReload.on("structure", structure);

      ws.simulateMessage({ type: "comments" });

      expect(content).not.toHaveBeenCalled();
      expect(structure).not.toHaveBeenCalled();
    });

    it.each([
      ["invalid JSON", "not json"],
      ["an unknown message type", JSON.stringify({ type: "ping" })],
    ])("ignores %s", (_, data) => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on("structure", callback);

      expect(() => ws.onmessage?.({ data })).not.toThrow();

      expect(callback).not.toHaveBeenCalled();
    });

    it("returns an unsubscribe function", () => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      const unsubscribe = liveReload.on("structure", callback);

      unsubscribe();
      ws.simulateMessage({ type: "structure" });

      expect(callback).not.toHaveBeenCalled();
    });

    it("unsubscribe does not clear a subsequently registered listener", () => {
      const { liveReload, ws } = connected();
      const callback1 = vi.fn();
      const callback2 = vi.fn();

      const unsubscribe1 = liveReload.on("structure", callback1);
      liveReload.on("structure", callback2);
      unsubscribe1();

      ws.simulateMessage({ type: "structure" });

      expect(callback1).not.toHaveBeenCalled();
      expect(callback2).toHaveBeenCalledTimes(1);
    });
  });

  describe("disconnect()", () => {
    it("closes WebSocket connection", () => {
      const { liveReload, ws } = connected();

      liveReload.disconnect();

      expect(ws.readyState).toBe(MockWebSocket.CLOSING);
    });

    it("does not reconnect after closing an open socket", () => {
      const { liveReload } = connected();

      liveReload.disconnect();
      vi.advanceTimersByTime(5000);

      expect(mockWebSocketInstances.length).toBe(1);
    });

    it("does not reconnect after closing a connecting socket", () => {
      const liveReload = createStore();
      liveReload.connect();

      liveReload.disconnect();
      vi.advanceTimersByTime(5000);

      expect(mockWebSocketInstances.length).toBe(1);
    });

    it("cancels pending reconnect", () => {
      const { liveReload, ws } = connected();
      ws.simulateClose();

      liveReload.disconnect();
      vi.advanceTimersByTime(5000);

      expect(mockWebSocketInstances.length).toBe(1);
    });

    it("handles disconnect when not connected", () => {
      const liveReload = createStore();
      // Should not throw
      expect(() => liveReload.disconnect()).not.toThrow();
    });
  });
});
