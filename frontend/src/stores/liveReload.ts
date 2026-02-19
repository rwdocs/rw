import { writable, get } from "svelte/store";
import { path, extractDocPath } from "./router";
import { navigation } from "./navigation";

interface LiveReloadState {
  connected: boolean;
  lastReload: string | null;
}

interface ReloadMessage {
  type: "content" | "structure";
  path: string;
}

function createLiveReloadStore() {
  const { subscribe, update } = writable<LiveReloadState>({
    connected: false,
    lastReload: null,
  });

  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let onReloadCallback: ((path: string) => void) | null = null;

  function connect() {
    if (ws?.readyState === WebSocket.OPEN) {
      return;
    }

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${protocol}//${window.location.host}/ws/live-reload`;

    ws = new WebSocket(url);

    ws.onopen = () => {
      update((state) => ({ ...state, connected: true }));
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Connected");
      }
    };

    ws.onclose = () => {
      update((state) => ({ ...state, connected: false }));
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Disconnected, reconnecting in 2s...");
      }
      scheduleReconnect();
    };

    ws.onerror = () => {
      ws?.close();
    };

    ws.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data) as ReloadMessage;
        if (message.type === "content") {
          handleContentReload(message.path);
        } else if (message.type === "structure") {
          handleStructureReload();
        }
      } catch (e) {
        if (import.meta.env.DEV) {
          console.warn("[LiveReload] Failed to parse message:", e);
        }
      }
    };
  }

  function handleContentReload(changedPath: string) {
    update((state) => ({ ...state, lastReload: changedPath }));

    if (import.meta.env.DEV) {
      console.log("[LiveReload] Content changed:", changedPath);
    }

    if (onReloadCallback && shouldReload(get(path), changedPath)) {
      onReloadCallback(changedPath);
    }
  }

  async function handleStructureReload() {
    if (import.meta.env.DEV) {
      console.log("[LiveReload] Structure changed");
    }

    await navigation.load({ bypassCache: true });

    const currentPath = get(path);
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }
  }

  function shouldReload(currentPath: string, changedPath: string): boolean {
    return extractDocPath(currentPath) === extractDocPath(changedPath);
  }

  function scheduleReconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
    }
    reconnectTimer = setTimeout(() => {
      connect();
    }, 2000);
  }

  function disconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.close();
      ws = null;
    }
  }

  return {
    subscribe,

    /** Start live reload connection */
    start() {
      connect();
    },

    /** Stop live reload connection */
    stop() {
      disconnect();
    },

    /** Register callback for reload events on current page */
    onReload(callback: (path: string) => void): () => void {
      onReloadCallback = callback;
      return () => {
        if (onReloadCallback === callback) {
          onReloadCallback = null;
        }
      };
    },
  };
}

export const liveReload = createLiveReloadStore();
