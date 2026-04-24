import Root from "./Root.svelte";
import Item from "./Item.svelte";

/**
 * Compound `<Menu.Root><Menu.Item/></Menu.Root>` API. Root is an anchored
 * overlay built on Popover; Item renders either an `<a>` (when `href` is set)
 * or a `<button>` with `role="menuitem"`. Opening the menu focuses the first
 * enabled item; arrow keys cycle focus; Escape or outside-click dismisses.
 */
export const Menu = { Root, Item };
