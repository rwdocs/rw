import { getContext, hasContext, setContext } from "svelte";

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

// Lenient context: `get()` returns `undefined` when there is no parent
// `<Menu.Root>`, so `<Menu.Item>` can render standalone. Svelte's built-in
// `createContext` throws on a missing provider, so Menu keeps an explicit
// `hasContext` guard instead. The Symbol key keeps this isolated from any
// other module that picks the same context name.
const key = Symbol("Menu");

export const menuContext = {
  set: (value: MenuContext): MenuContext => setContext(key, value),
  get: (): MenuContext | undefined => (hasContext(key) ? getContext<MenuContext>(key) : undefined),
};
