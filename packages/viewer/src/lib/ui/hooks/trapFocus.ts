const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

// Focusable descendants in DOM order. No visibility filtering: the drawer
// panel renders only currently-visible controls (collapsed nav items are
// removed from the DOM, not hidden), and jsdom reports no layout, so an
// offsetParent/getClientRects filter would wrongly drop everything in tests.
function focusableWithin(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
}

/**
 * Svelte attachment that turns its host element into a modal focus trap.
 *
 * On attach it saves the currently-focused element and moves focus to the host
 * (give the host `tabindex="-1"` so it can receive focus). While attached, Tab
 * and Shift+Tab cycle within the host's focusable descendants: Tab on the last
 * wraps to the first and Shift+Tab on the first wraps to the last. While focus
 * still rests on the host itself (the just-opened state), the first Tab enters
 * the first descendant and the first Shift+Tab enters the last. The returned
 * cleanup restores focus to the saved element and removes the listener.
 *
 * Tab wrapping is fully contained here: the keydown listener lives on `node`,
 * so it only sees Tab while focus is on the host or a descendant, and it never
 * lets Tab move focus out. Callers should still keep the surrounding content
 * non-focusable (e.g. `inert`) — not for Tab, but so a click or programmatic
 * `.focus()` cannot park focus on the obscured content behind the modal.
 *
 * Because consumers render the host only while the modal is open, attach
 * corresponds to "opened" and cleanup to "closed" — no `open` flag is threaded
 * in. Mirrors the `dismissOnInteraction` attachment in `Popover.svelte`.
 */
export function trapFocus(node: HTMLElement): () => void {
  const previouslyFocused =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;

  node.focus();

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;

    const focusable = focusableWithin(node);
    if (focusable.length === 0) {
      // Nothing inside to land on; keep focus pinned to the host.
      event.preventDefault();
      node.focus();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;

    // `active === node` is the just-opened state (focus on the host itself);
    // treat it as being at both edges so the first Tab/Shift+Tab enters the
    // ends of the list instead of escaping.
    if (event.shiftKey) {
      if (active === first || active === node) {
        event.preventDefault();
        last.focus();
      }
    } else {
      if (active === last || active === node) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  node.addEventListener("keydown", onKeydown);

  return () => {
    node.removeEventListener("keydown", onKeydown);
    if (previouslyFocused?.isConnected) {
      previouslyFocused.focus();
    }
  };
}
