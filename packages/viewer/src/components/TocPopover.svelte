<script lang="ts">
  import { getRwContext } from "../lib/context";
  import TocSidebar from "./TocSidebar.svelte";
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
  }

  let { toc }: Props = $props();

  const { ui } = getRwContext();

  let popoverEl: HTMLDivElement | undefined = $state();

  function handleClickOutside(event: MouseEvent) {
    if (popoverEl && !popoverEl.contains(event.target as Node)) {
      ui.closeTocPopover();
    }
  }

  $effect(() => {
    if ($ui.tocPopoverOpen) {
      document.addEventListener("click", handleClickOutside, true);
      return () => document.removeEventListener("click", handleClickOutside, true);
    }
  });

  $effect(() => {
    if (!$ui.tocPopoverOpen) return;
    function handleKeydown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        ui.closeTocPopover();
      }
    }
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

<div class="relative" bind:this={popoverEl}>
  <button
    onclick={ui.toggleTocPopover}
    class="
      flex size-8 cursor-pointer items-center justify-center rounded-sm border border-gray-200
      bg-white text-gray-500
      hover:border-gray-300 hover:text-gray-700
      dark:border-neutral-600 dark:bg-neutral-800 dark:text-neutral-400
      dark:hover:border-neutral-500 dark:hover:text-neutral-300
    "
    class:border-gray-300={$ui.tocPopoverOpen}
    class:dark:border-neutral-500={$ui.tocPopoverOpen}
    aria-label="Table of contents"
    aria-expanded={$ui.tocPopoverOpen}
  >
    <svg
      class="size-4"
      fill="currentColor"
      viewBox="0 0 24 24"
      stroke="currentColor"
      stroke-width="2.5"
    >
      <path stroke-linecap="round" stroke-linejoin="round" d="M8 6h13M8 12h13M8 18h13" />
      <circle cx="3" cy="6" r="1.5" stroke="none" />
      <circle cx="3" cy="12" r="1.5" stroke="none" />
      <circle cx="3" cy="18" r="1.5" stroke="none" />
    </svg>
  </button>

  {#if $ui.tocPopoverOpen}
    <nav
      class="
        absolute top-full right-0 z-40 mt-2 max-h-[min(24rem,calc(100cqb-5rem))] w-64
        overflow-y-auto rounded-lg border border-gray-200 bg-white p-4 shadow-lg
        dark:border-neutral-600 dark:bg-neutral-800
      "
      aria-label="Table of contents"
    >
      <TocSidebar {toc} onnavigate={ui.closeTocPopover} />
    </nav>
  {/if}
</div>
