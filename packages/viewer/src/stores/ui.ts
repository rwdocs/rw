import { writable } from "svelte/store";
import type { Readable } from "svelte/store";

export interface UiStore extends Readable<{ mobileMenuOpen: boolean; tocPopoverOpen: boolean }> {
  openMobileMenu(): void;
  closeMobileMenu(): void;
  toggleTocPopover(): void;
  closeTocPopover(): void;
}

export function createUiStore(): UiStore {
  const { subscribe, update } = writable({
    mobileMenuOpen: false,
    tocPopoverOpen: false,
  });

  return {
    subscribe,
    openMobileMenu() {
      update((state) => ({ ...state, mobileMenuOpen: true }));
    },
    closeMobileMenu() {
      update((state) => ({ ...state, mobileMenuOpen: false }));
    },
    toggleTocPopover() {
      update((state) => ({ ...state, tocPopoverOpen: !state.tocPopoverOpen }));
    },
    closeTocPopover() {
      update((state) => ({ ...state, tocPopoverOpen: false }));
    },
  };
}
