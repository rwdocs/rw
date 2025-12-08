import { writable, get } from "svelte/store";
import { path } from "./router";
import { navigation } from "./navigation";

interface LiveReloadState {
  connected: boolean;
  lastReload: string | null;
}

interface ReloadMessage {
  type: "reload";
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
        if (message.type === "reload") {
          handleReload(message.path);
        }
      } catch (e) {
        if (import.meta.env.DEV) {
          console.warn("[LiveReload] Failed to parse message:", e);
        }
      }
    };
  }

  function handleReload(changedPath: string) {
    update((state) => ({ ...state, lastReload: changedPath }));

    if (import.meta.env.DEV) {
      console.log("[LiveReload] File changed:", changedPath);
    }

    navigation.load({ bypassCache: true });

    const currentPath = get(path);
    if (onReloadCallback && shouldReload(currentPath, changedPath)) {
      onReloadCallback(changedPath);
    }
  }

  function shouldReload(currentPath: string, changedPath: string): boolean {
    const normalizedCurrent = currentPath.replace(/^\/docs\/?/, "");
    const normalizedChanged = changedPath.replace(/^\/docs\/?/, "");

    return normalizedCurrent === normalizedChanged;
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
