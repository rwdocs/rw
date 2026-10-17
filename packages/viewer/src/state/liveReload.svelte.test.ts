import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { LiveReload } from "./liveReload.svelte";
import type { Router } from "./router.svelte";

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

  close() {
    if (this.readyState === MockWebSocket.CLOSED) {
      return; // Already closed, don't fire onclose again
    }
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  // Test helpers
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
  });

  describe("connection events", () => {
    it("schedules reconnect on close", () => {
      const { ws } = connected();
      ws.close();

      expect(mockWebSocketInstances.length).toBe(1);

      vi.advanceTimersByTime(2000);

      expect(mockWebSocketInstances.length).toBe(2);
    });

    it("closes connection on error", () => {
      const { ws } = connected();

      ws.simulateError();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });
  });

  describe("on()", () => {
    it.each(["structure", "comments"] as const)("calls the %s listener", (type) => {
      const { liveReload, ws } = connected();
      const callback = vi.fn();
      liveReload.on(type, callback);

      ws.simulateMessage({ type, path: "/guide" });

      expect(callback).toHaveBeenCalledTimes(1);
    });

    it("calls the content listener for the current page", () => {
      // Router path is /docs/guide; extractDocPath strips the leading slash on both sides.
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

    it("routes each message only to the listener of its type", () => {
      const { liveReload, ws } = connected();
      const content = vi.fn();
      const comments = vi.fn();
      liveReload.on("content", content);
      liveReload.on("comments", comments);

      ws.simulateMessage({ type: "structure", path: "/docs/guide" });

      expect(content).not.toHaveBeenCalled();
      expect(comments).not.toHaveBeenCalled();
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
      ws.simulateMessage({ type: "structure", path: "/guide" });

      expect(callback).not.toHaveBeenCalled();
    });

    it("unsubscribe does not clear a subsequently registered listener", () => {
      const { liveReload, ws } = connected();
      const callback1 = vi.fn();
      const callback2 = vi.fn();

      const unsubscribe1 = liveReload.on("structure", callback1);
      liveReload.on("structure", callback2);
      unsubscribe1();

      ws.simulateMessage({ type: "structure", path: "/guide" });

      expect(callback1).not.toHaveBeenCalled();
      expect(callback2).toHaveBeenCalledTimes(1);
    });
  });

  describe("disconnect()", () => {
    it("closes WebSocket connection", () => {
      const { liveReload, ws } = connected();

      liveReload.disconnect();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });

    it("cancels pending reconnect", () => {
      const liveReload = createStore();
      liveReload.connect();
      const initialCount = mockWebSocketInstances.length;

      // Trigger close which schedules reconnect
      mockWebSocketInstances[0].close();

      // Disconnect should cancel the reconnect timer
      liveReload.disconnect();

      vi.advanceTimersByTime(2000);

      // Should not have created new connection after disconnect
      expect(mockWebSocketInstances.length).toBe(initialCount);
    });

    it("handles disconnect when not connected", () => {
      const liveReload = createStore();
      // Should not throw
      expect(() => liveReload.disconnect()).not.toThrow();
    });
  });
});
