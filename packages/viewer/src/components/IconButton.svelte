<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    onclick: () => void;
    "aria-label": string;
    "aria-expanded"?: boolean;
    active?: boolean;
    class?: string;
    children: Snippet;
  }

  let {
    onclick,
    "aria-label": ariaLabel,
    "aria-expanded": ariaExpanded,
    active = false,
    class: extraClass = "",
    children,
  }: Props = $props();
</script>

<button
  {onclick}
  class="
    flex size-8 cursor-pointer items-center justify-center rounded-sm border border-gray-200
    bg-white text-gray-500
    hover:border-gray-300 hover:bg-gray-50 hover:text-gray-700
    focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-blue-500
    active:bg-gray-100
    dark:border-neutral-600 dark:bg-neutral-800 dark:text-neutral-400
    dark:hover:border-neutral-500 dark:hover:bg-neutral-700 dark:hover:text-neutral-300
    dark:active:bg-neutral-600
    {extraClass}
  "
  class:border-gray-300={active}
  class:dark:border-neutral-500={active}
  aria-label={ariaLabel}
  aria-expanded={ariaExpanded}
>
  {@render children()}
</button>
