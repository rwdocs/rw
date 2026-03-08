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

  describe("initial state", () => {
    it("starts disconnected", () => {
      const liveReload = createStore();
      expect(liveReload.connected).toBe(false);
      expect(liveReload.lastReload).toBeNull();
    });
  });

  describe("start()", () => {
    it("creates WebSocket connection", () => {
      const liveReload = createStore();
      liveReload.start();

      expect(mockWebSocketInstances.length).toBe(1);
      expect(mockWebSocketInstances[0].url).toBe("ws://localhost:7979/ws/live-reload");
    });

    it("uses wss protocol for https", () => {
      vi.stubGlobal("location", {
        protocol: "https:",
        host: "localhost:7979",
      });

      const liveReload = createStore();
      liveReload.start();

      const lastInstance = mockWebSocketInstances[mockWebSocketInstances.length - 1];
      expect(lastInstance.url).toBe("wss://localhost:7979/ws/live-reload");

      liveReload.stop();
    });

    it("does not create duplicate connection if already open", () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      liveReload.start();

      expect(mockWebSocketInstances.length).toBe(1);
    });
  });

  describe("connection events", () => {
    it("sets connected to true on open", () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      expect(liveReload.connected).toBe(true);
    });

    it("sets connected to false on close", () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      mockWebSocketInstances[0].close();

      expect(liveReload.connected).toBe(false);
    });

    it("schedules reconnect on close", () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      mockWebSocketInstances[0].close();

      expect(mockWebSocketInstances.length).toBe(1);

      vi.advanceTimersByTime(2000);

      expect(mockWebSocketInstances.length).toBe(2);
    });

    it("closes connection on error", () => {
      const liveReload = createStore();
      liveReload.start();
      const ws = mockWebSocketInstances[0];
      ws.simulateOpen();

      ws.simulateError();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });
  });

  describe("message handling", () => {
    it("updates lastReload on content message", async () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(liveReload.lastReload).toBe("/guide");
    });

    it("calls onStructureReload callback on structure message", async () => {
      const callback = vi.fn();
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onStructureReload(callback);

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(callback).toHaveBeenCalled();
    });

    it("ignores invalid JSON messages", async () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].onmessage?.({ data: "not json" });

      await vi.runAllTimersAsync();

      expect(liveReload.lastReload).toBeNull();
    });

    it("ignores unknown message types", async () => {
      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "ping" });

      await vi.runAllTimersAsync();

      expect(liveReload.lastReload).toBeNull();
    });
  });

  describe("onReload callback", () => {
    it("calls callback on content event for current page", async () => {
      // Router path is /docs/guide, extractDocPath strips leading slash
      // so extractDocPath("/docs/guide") = "docs/guide"
      // The changed path "/docs/guide" also gives "docs/guide"
      const callback = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onReload(callback);

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/docs/guide" });

      await vi.runAllTimersAsync();

      expect(callback).toHaveBeenCalledWith("/docs/guide");
    });

    it("does not call callback on content event for different page", async () => {
      const callback = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onReload(callback);

      // Current path is /docs/guide (extractDocPath -> "docs/guide")
      // Changed path is /docs/other (extractDocPath -> "docs/other")
      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/docs/other" });

      await vi.runAllTimersAsync();

      expect(callback).not.toHaveBeenCalled();
    });

    it("does not call callback on structure event", async () => {
      const callback = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onReload(callback);

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/docs/guide" });

      await vi.runAllTimersAsync();

      expect(callback).not.toHaveBeenCalled();
    });

    it("returns unsubscribe function", async () => {
      const callback = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      const unsubscribe = liveReload.onReload(callback);

      unsubscribe();

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/docs/guide" });

      await vi.runAllTimersAsync();

      expect(callback).not.toHaveBeenCalled();
    });

    it("unsubscribe only removes matching callback", async () => {
      const callback1 = vi.fn();
      const callback2 = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      const unsubscribe1 = liveReload.onReload(callback1);
      liveReload.onReload(callback2);

      unsubscribe1();

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/docs/guide" });

      await vi.runAllTimersAsync();

      expect(callback2).toHaveBeenCalled();
    });
  });

  describe("onStructureReload callback", () => {
    it("returns unsubscribe function", async () => {
      const callback = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      const unsubscribe = liveReload.onStructureReload(callback);

      unsubscribe();

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(callback).not.toHaveBeenCalled();
    });

    it("unsubscribe only removes matching callback", async () => {
      const callback1 = vi.fn();
      const callback2 = vi.fn();

      const liveReload = createStore();
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      const unsubscribe1 = liveReload.onStructureReload(callback1);
      liveReload.onStructureReload(callback2);

      unsubscribe1();

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(callback2).toHaveBeenCalled();
    });
  });

  describe("stop()", () => {
    it("closes WebSocket connection", () => {
      const liveReload = createStore();
      liveReload.start();
      const ws = mockWebSocketInstances[0];
      ws.simulateOpen();

      liveReload.stop();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });

    it("cancels pending reconnect", () => {
      const liveReload = createStore();
      liveReload.start();
      const initialCount = mockWebSocketInstances.length;

      // Trigger close which schedules reconnect
      mockWebSocketInstances[0].close();

      // Stop should cancel the reconnect timer
      liveReload.stop();

      vi.advanceTimersByTime(2000);

      // Should not have created new connection after stop
      expect(mockWebSocketInstances.length).toBe(initialCount);
    });

    it("handles stop when not connected", () => {
      const liveReload = createStore();
      // Should not throw
      expect(() => liveReload.stop()).not.toThrow();
    });
  });
});
