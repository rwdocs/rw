import { on } from "svelte/events";

/**
 * Listen for keydown on behalf of the overlay `el`. Under `mountRw()` the rest
 * of the page belongs to the host, so a key pressed in a host field is skipped:
 * Escape there must not close the overlay and pull focus back. A key with
 * nothing focused still counts — it is what follows a click on plain text, such
 * as a comment highlight.
 */
export function onViewerKeydown(el: Element, handler: (event: KeyboardEvent) => void): () => void {
  const root = el.closest("[data-rw-viewer]");
  return on(window, "keydown", (event) => {
    if (!root || event.target === document.body || event.composedPath().includes(root)) {
      handler(event);
    }
  });
}
