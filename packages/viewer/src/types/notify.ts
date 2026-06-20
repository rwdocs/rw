export type NotifyIntent = "info" | "success" | "warning" | "error";

export interface Notification {
  intent: NotifyIntent;
  message: string;
}

export type NotifyFn = (notification: Notification) => void;

/** A queued notification; `id` is its dismissal handle. */
export interface Toast extends Notification {
  id: number;
}
