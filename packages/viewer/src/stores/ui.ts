import { writable } from "svelte/store";
import type { Readable } from "svelte/store";

export interface UiStore extends Readable<{ mobileMenuOpen: boolean }> {
  openMobileMenu(): void;
  closeMobileMenu(): void;
}

export function createUiStore(): UiStore {
  const { subscribe, update } = writable({ mobileMenuOpen: false });

  return {
    subscribe,
    openMobileMenu() {
      update((state) => ({ ...state, mobileMenuOpen: true }));
    },
    closeMobileMenu() {
      update((state) => ({ ...state, mobileMenuOpen: false }));
    },
  };
}
