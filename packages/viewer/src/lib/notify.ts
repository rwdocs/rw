import type { Notification, NotifyFn } from "../types/notify";

/**
 * Returns a NotifyFn wired to the active delivery channel: the host-supplied
 * `onNotify` when present (e.g. a Backstage alert adapter), otherwise the
 * built-in Toaster via `ui.pushToast`. The returned function is stable.
 */
export function createNotify(
  ui: { pushToast: (n: Notification) => void },
  onNotify?: NotifyFn,
): NotifyFn {
  return onNotify ?? ((n) => ui.pushToast(n));
}
