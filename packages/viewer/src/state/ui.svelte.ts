import type { Notification, Toast } from "../types/notify";

export class Ui {
  mobileMenuOpen = $state(false);
  /** True when the layout container is narrower than the comments breakpoint, so
   *  inline-comment threads show in the CommentPopover rather than the margin
   *  aside. Written by Layout from a ResizeObserver; read by PageContent. */
  narrow = $state(false);
  toasts = $state.raw<Toast[]>([]);
  #nextToastId = 0;

  openMobileMenu = () => {
    this.mobileMenuOpen = true;
  };

  closeMobileMenu = () => {
    this.mobileMenuOpen = false;
  };

  /** Enqueue a toast. Identical (intent + message) toasts are de-duped so a
   *  repeated failed action (e.g. mashing ⌘+Enter while offline) doesn't stack
   *  a tower of the same message. Returns the toast id (pass to `dismissToast`). */
  pushToast = (n: Notification): number => {
    const existing = this.toasts.find((t) => t.intent === n.intent && t.message === n.message);
    if (existing) return existing.id;
    const toast: Toast = { id: this.#nextToastId++, intent: n.intent, message: n.message };
    this.toasts = [...this.toasts, toast];
    return toast.id;
  };

  dismissToast = (id: number) => {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  };
}
