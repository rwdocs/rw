<script lang="ts">
  import type { Snippet } from "svelte";
  import { menuContext } from "./context";

  interface Props {
    /** Renders an `<a href>` when set, a `<button>` otherwise. */
    href?: string;
    onclick?: (event: MouseEvent) => void;
    disabled?: boolean;
    children: Snippet;
    class?: string;
  }

  let { href, onclick, disabled = false, class: extraClass = "", children }: Props = $props();

  // Lenient lookup: Menu.Item renders fine without a parent Menu.Root
  // (e.g. when included standalone in a documentation showcase). When
  // nested in Menu.Root, close() dismisses the menu after activation and
  // isTabbable() drives the roving tabindex.
  const ctx = menuContext.get();

  let itemEl: HTMLElement | undefined = $state();

  // Roving tabindex: the one active item is tabindex=0, others are -1. With
  // no parent Menu.Root, fall back to `undefined` so standalone items keep
  // their native tab behavior.
  const tabIndex = $derived(ctx ? (ctx.isTabbable(itemEl) ? 0 : -1) : undefined);

  // Only emit role="menuitem" when wrapped in a Menu.Root — otherwise the
  // element would claim menuitem semantics with no enclosing role="menu".
  const itemRole = ctx ? "menuitem" : undefined;

  function handleClick(event: MouseEvent) {
    if (disabled) {
      event.preventDefault();
      return;
    }
    onclick?.(event);
    ctx?.close();
  }

  const baseClass = `
    block w-full cursor-pointer px-3 py-1.5 text-left text-sm text-fg-muted
    transition-colors
    hover:bg-bg-subtle hover:text-fg-default
    focus:bg-bg-subtle focus:text-fg-default focus:outline-none
    aria-disabled:cursor-not-allowed aria-disabled:opacity-50
  `;
</script>

<svelte:element
  this={href !== undefined ? "a" : "button"}
  bind:this={itemEl}
  {href}
  type={href !== undefined ? undefined : "button"}
  role={itemRole}
  tabindex={tabIndex}
  aria-disabled={disabled || undefined}
  onclick={handleClick}
  class="{baseClass} {extraClass}"
>
  {@render children()}
</svelte:element>
