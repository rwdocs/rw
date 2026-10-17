import { extractDocPath } from "./router.svelte";
import type { Router } from "./router.svelte";

type ReloadMessage =
  { type: "content"; path: string } | { type: "structure"; path: string } | { type: "comments" };

type ReloadType = ReloadMessage["type"];

export class LiveReload {
  private router: Router;
  private ws: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private listeners = new Map<ReloadType, () => void>();

  constructor(deps: { router: Router }) {
    this.router = deps.router;
  }

  /** Register the single listener for `type`, replacing any previous one. A
   *  `content` listener fires only for the page on screen. */
  on = (type: ReloadType, callback: () => void): (() => void) => {
    this.listeners.set(type, callback);
    return () => {
      if (this.listeners.get(type) === callback) {
        this.listeners.delete(type);
      }
    };
  };

  connect() {
    // A connecting socket either opens or closes and schedules a reconnect.
    if (this.ws && this.ws.readyState <= WebSocket.OPEN) {
      return;
    }

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${protocol}//${window.location.host}/ws/live-reload`;

    // Events of a replaced socket keep arriving after disconnect(); each
    // handler acts on its own socket and only while it is still current.
    const ws = new WebSocket(url);
    this.ws = ws;

    ws.onopen = () => {
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Connected");
      }
    };

    ws.onclose = () => {
      if (this.ws !== ws) return;
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Disconnected, reconnecting in 2s...");
      }
      this.scheduleReconnect();
    };

    ws.onerror = () => {
      ws.close();
    };

    ws.onmessage = (event: MessageEvent<unknown>) => {
      if (this.ws !== ws) return;
      // `MessageEvent.data` is `any`, and a socket can deliver Blob or
      // ArrayBuffer frames as well as text. The server only ever sends JSON
      // text, so anything else is dropped rather than fed to JSON.parse.
      if (typeof event.data !== "string") return;
      let message: ReloadMessage;
      try {
        message = JSON.parse(event.data) as ReloadMessage;
      } catch (e) {
        if (import.meta.env.DEV) {
          console.warn("[LiveReload] Failed to parse message:", e);
        }
        return;
      }
      if (import.meta.env.DEV) {
        console.log("[LiveReload] Changed:", message);
      }
      // Dispatch on literal keys: `message.type` is unvalidated wire data.
      switch (message.type) {
        case "content":
          if (extractDocPath(this.router.path) === extractDocPath(message.path)) {
            this.listeners.get("content")?.();
          }
          break;
        case "structure":
          this.listeners.get("structure")?.();
          break;
        case "comments":
          this.listeners.get("comments")?.();
          break;
      }
    };
  }

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
    }
    this.reconnectTimer = setTimeout(() => {
      this.connect();
    }, 2000);
  }
}
