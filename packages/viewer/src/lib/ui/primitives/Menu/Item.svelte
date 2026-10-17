<script lang="ts">
  import type { Snippet } from "svelte";
  import { getMenuContext } from "./context";

  interface Props {
    /** Renders an `<a href>` when set, a `<button>` otherwise. */
    href?: string;
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  }

  let { href, onclick, children }: Props = $props();

  const ctx = getMenuContext();

  let itemEl: HTMLElement | undefined = $state();

  function handleClick(event: MouseEvent) {
    onclick?.(event);
    ctx.close();
  }
</script>

<svelte:element
  this={href !== undefined ? "a" : "button"}
  bind:this={itemEl}
  {href}
  type={href !== undefined ? undefined : "button"}
  role="menuitem"
  tabindex={ctx.isTabbable(itemEl) ? 0 : -1}
  onclick={handleClick}
  class="
    block w-full cursor-pointer px-3 py-1.5 text-left text-sm text-fg-muted transition-colors
    hover:bg-bg-subtle hover:text-fg-default
    focus:bg-bg-subtle focus:text-fg-default focus:outline-none
  "
>
  {@render children()}
</svelte:element>
