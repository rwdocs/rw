import { writable } from "svelte/store";

/** Mobile navigation drawer state */
export const mobileMenuOpen = writable(false);

export function openMobileMenu() {
  mobileMenuOpen.set(true);
}

export function closeMobileMenu() {
  mobileMenuOpen.set(false);
}

export function toggleMobileMenu() {
  mobileMenuOpen.update((open) => !open);
}
