import { createContextKey } from "../../context";

/**
 * Shared state `<Menu.Root>` publishes for `<Menu.Item>` descendants:
 * - `close()` dismisses the menu after activation, so items don't need to
 *   know about the parent's `open` binding.
 * - `isTabbable(el)` drives roving tabindex — the currently-active menuitem
 *   is the single tab stop, so Tab from outside lands on one well-defined
 *   item instead of walking through every interior menuitem.
 */
export interface MenuContext {
  close(): void;
  isTabbable(el: HTMLElement | undefined): boolean;
}

export const menuContext = createContextKey<MenuContext>("Menu");
