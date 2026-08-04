/**
 * Manages click-outside and Escape key dismissal for popovers/dropdowns.
 * Registers listeners only while open, and cleans up automatically via $effect return.
 *
 * Usage in a Svelte 5 component:
 *   $effect(() => dismissible(isOpen, containerEl, closeFunction));
 */
import { on } from "svelte/events";

export function dismissible(
  isOpen: boolean,
  containerEl: HTMLElement | undefined,
  close: () => void,
): (() => void) | undefined {
  if (!isOpen) return;

  function handleClick(event: MouseEvent) {
    if (containerEl && !containerEl.contains(event.target as Node)) {
      close();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  const offClick = on(document, "click", handleClick, { capture: true });
  const offKeydown = on(window, "keydown", handleKeydown);

  return () => {
    offClick();
    offKeydown();
  };
}
