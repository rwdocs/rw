import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { get } from "svelte/store";

// Mock dependencies before importing liveReload
vi.mock("./router", () => ({
  path: {
    subscribe: vi.fn((cb) => {
      cb("/docs/guide");
      return () => {};
    }),
  },
  extractDocPath: vi.fn((p: string) => p.replace(/^\/docs/, "") || "/"),
}));

vi.mock("./navigation", () => ({
  navigation: {
    load: vi.fn().mockResolvedValue(undefined),
    expandOnlyTo: vi.fn(),
  },
}));

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

describe("liveReload store", () => {
  let liveReload: typeof import("./liveReload").liveReload;
  let mockNavigation: { load: ReturnType<typeof vi.fn>; expandOnlyTo: ReturnType<typeof vi.fn> };
  let mockExtractDocPath: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    vi.useFakeTimers();
    mockWebSocketInstances = [];

    // Reset modules to get fresh store instance
    vi.resetModules();

    // Setup WebSocket mock
    vi.stubGlobal("WebSocket", MockWebSocket);

    // Setup location mock
    vi.stubGlobal("location", {
      protocol: "http:",
      host: "localhost:7979",
    });

    // Import fresh module
    const module = await import("./liveReload");
    liveReload = module.liveReload;

    // Get mocked dependencies
    const routerMock = await import("./router");
    mockExtractDocPath = routerMock.extractDocPath as ReturnType<typeof vi.fn>;

    const navMock = await import("./navigation");
    mockNavigation = navMock.navigation as unknown as {
      load: ReturnType<typeof vi.fn>;
      expandOnlyTo: ReturnType<typeof vi.fn>;
    };
  });

  afterEach(() => {
    liveReload.stop();
    vi.unstubAllGlobals();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  describe("initial state", () => {
    it("starts disconnected", () => {
      const state = get(liveReload);
      expect(state.connected).toBe(false);
      expect(state.lastReload).toBeNull();
    });
  });

  describe("start()", () => {
    it("creates WebSocket connection", () => {
      liveReload.start();

      expect(mockWebSocketInstances.length).toBe(1);
      expect(mockWebSocketInstances[0].url).toBe("ws://localhost:7979/ws/live-reload");
    });

    it("uses wss protocol for https", async () => {
      vi.stubGlobal("location", {
        protocol: "https:",
        host: "localhost:7979",
      });

      // Reset and reimport to pick up new location
      vi.resetModules();
      const module = await import("./liveReload");

      module.liveReload.start();

      const lastInstance = mockWebSocketInstances[mockWebSocketInstances.length - 1];
      expect(lastInstance.url).toBe("wss://localhost:7979/ws/live-reload");

      module.liveReload.stop();
    });

    it("does not create duplicate connection if already open", () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      liveReload.start();

      expect(mockWebSocketInstances.length).toBe(1);
    });
  });

  describe("connection events", () => {
    it("sets connected to true on open", () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      const state = get(liveReload);
      expect(state.connected).toBe(true);
    });

    it("sets connected to false on close", () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      mockWebSocketInstances[0].close();

      const state = get(liveReload);
      expect(state.connected).toBe(false);
    });

    it("schedules reconnect on close", () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      mockWebSocketInstances[0].close();

      expect(mockWebSocketInstances.length).toBe(1);

      vi.advanceTimersByTime(2000);

      expect(mockWebSocketInstances.length).toBe(2);
    });

    it("closes connection on error", () => {
      liveReload.start();
      const ws = mockWebSocketInstances[0];
      ws.simulateOpen();

      ws.simulateError();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });
  });

  describe("message handling", () => {
    it("updates lastReload on content message", async () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/guide" });

      await vi.runAllTimersAsync();

      const state = get(liveReload);
      expect(state.lastReload).toBe("/guide");
    });

    it("does not reload navigation on content message", async () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(mockNavigation.load).not.toHaveBeenCalled();
    });

    it("reloads navigation on structure message", async () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(mockNavigation.load).toHaveBeenCalledWith({ bypassCache: true });
    });

    it("expands navigation to current path after structure reload", async () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/guide" });

      await vi.runAllTimersAsync();

      expect(mockNavigation.expandOnlyTo).toHaveBeenCalledWith("/docs/guide");
    });

    it("ignores invalid JSON messages", async () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].onmessage?.({ data: "not json" });

      await vi.runAllTimersAsync();

      const state = get(liveReload);
      expect(state.lastReload).toBeNull();
    });

    it("ignores unknown message types", async () => {
      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();

      mockWebSocketInstances[0].simulateMessage({ type: "ping" });

      await vi.runAllTimersAsync();

      const state = get(liveReload);
      expect(state.lastReload).toBeNull();
    });
  });

  describe("onReload callback", () => {
    it("calls callback on content event for current page", async () => {
      const callback = vi.fn();
      mockExtractDocPath.mockImplementation((p: string) => p.replace(/^\/docs/, "") || "/");

      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onReload(callback);

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/docs/guide" });

      await vi.runAllTimersAsync();

      expect(callback).toHaveBeenCalledWith("/docs/guide");
    });

    it("does not call callback on content event for different page", async () => {
      const callback = vi.fn();
      mockExtractDocPath.mockReturnValueOnce("/guide").mockReturnValueOnce("/other");

      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onReload(callback);

      mockWebSocketInstances[0].simulateMessage({ type: "content", path: "/docs/other" });

      await vi.runAllTimersAsync();

      expect(callback).not.toHaveBeenCalled();
    });

    it("does not call callback on structure event", async () => {
      const callback = vi.fn();
      mockExtractDocPath.mockReturnValue("/guide");

      liveReload.start();
      mockWebSocketInstances[0].simulateOpen();
      liveReload.onReload(callback);

      mockWebSocketInstances[0].simulateMessage({ type: "structure", path: "/docs/guide" });

      await vi.runAllTimersAsync();

      expect(callback).not.toHaveBeenCalled();
    });

    it("returns unsubscribe function", async () => {
      const callback = vi.fn();
      mockExtractDocPath.mockReturnValue("/guide");

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
      mockExtractDocPath.mockReturnValue("/guide");

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

  describe("stop()", () => {
    it("closes WebSocket connection", () => {
      liveReload.start();
      const ws = mockWebSocketInstances[0];
      ws.simulateOpen();

      liveReload.stop();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });

    it("cancels pending reconnect", () => {
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
      // Should not throw
      expect(() => liveReload.stop()).not.toThrow();
    });
  });
});
