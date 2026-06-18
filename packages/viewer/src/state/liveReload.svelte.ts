import { extractDocPath } from "./router.svelte";
import type { Router } from "./router.svelte";

type ReloadMessage =
  | { type: "content"; path: string }
  | { type: "structure"; path: string }
  | { type: "comments" };

export class LiveReload {
  connected = $state(false);
  lastReload = $state<string | null>(null);

  private router: Router;
  private ws: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private onReloadCallback: ((path: string) => void) | null = null;
  private onStructureReloadCallback: (() => void) | null = null;
  private onCommentsReloadCallback: (() => void) | null = null;

  constructor(deps: { router: Router }) {
    this.router = deps.router;
  }

  start = () => {
    this.connect();
  };

  stop = () => {
    this.disconnect();
  };

  onReload = (callback: (path: string) => void): (() => void) => {
    this.onReloadCallback = callback;
    return () => {
      if (this.onReloadCallback === callback) {
        this.onReloadCallback = null;
      }
    };
  };

  onStructureReload = (callback: () => void): (() => void) => {
    this.onStructureReloadCallback = callback;
    return () => {
      if (this.onStructureReloadCallback === callback) {
        this.onStructureReloadCallback = null;
      }
    };
  };

  onCommentsReload = (callback: () => void): (() => void) => {
    this.onCommentsReloadCallback = callback;
    return () => {
      if (this.onCommentsReloadCallback === callback) {
        this.onCommentsReloadCallback = null;
      }
    };
  };

  private connect() {
    if (this.ws?.readyState === WebSocket.OPEN) {
      return;
    }

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${protocol}//${window.location.host}/ws/live-reload`;

    this.ws = new WebSocket(url);

    this.ws.onopen = () => {
      this.connected = true;
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Connected");
      }
    };

    this.ws.onclose = () => {
      this.connected = false;
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Disconnected, reconnecting in 2s...");
      }
      this.scheduleReconnect();
    };

    this.ws.onerror = () => {
      this.ws?.close();
    };

    this.ws.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data) as ReloadMessage;
        if (message.type === "content") {
          this.handleContentReload(message.path);
        } else if (message.type === "structure") {
          this.handleStructureReload();
        } else if (message.type === "comments") {
          this.handleCommentsReload();
        }
      } catch (e) {
        if (import.meta.env.DEV) {
          console.warn("[LiveReload] Failed to parse message:", e);
        }
      }
    };
  }

  private handleContentReload(changedPath: string) {
    this.lastReload = changedPath;

    if (import.meta.env.DEV) {
      console.log("[LiveReload] Content changed:", changedPath);
    }

    if (this.onReloadCallback && this.shouldReload(this.router.path, changedPath)) {
      this.onReloadCallback(changedPath);
    }
  }

  private handleStructureReload() {
    if (import.meta.env.DEV) {
      console.log("[LiveReload] Structure changed");
    }

    this.onStructureReloadCallback?.();
  }

  private handleCommentsReload() {
    if (import.meta.env.DEV) {
      console.log("[LiveReload] Comments changed");
    }
    this.onCommentsReloadCallback?.();
  }

  private shouldReload(currentPath: string, changedPath: string): boolean {
    return extractDocPath(currentPath) === extractDocPath(changedPath);
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
    }
    this.reconnectTimer = setTimeout(() => {
      this.connect();
    }, 2000);
  }

  private disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
