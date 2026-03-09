<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { dismissible } from "../lib/dismissible";
  import IconButton from "./IconButton.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
  }

  let { toc }: Props = $props();

  const { router } = getRwContext();

  let open = $state(false);
  let popoverEl: HTMLDivElement | undefined = $state();

  function toggle() {
    open = !open;
  }

  function close() {
    open = false;
  }

  // Close on navigation
  $effect(() => {
    void router.path;
    close();
  });

  $effect(() => dismissible(open, popoverEl, close));
</script>

<div class="relative" bind:this={popoverEl}>
  <IconButton onclick={toggle} aria-label="Table of contents" aria-expanded={open} active={open}>
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
  </IconButton>

  {#if open}
    <nav
      class="
        absolute top-full right-0 z-40 mt-2 max-h-[min(24rem,calc(100cqb-5rem))] w-64
        overflow-y-auto rounded-lg border border-gray-200 bg-white p-4 shadow-lg
        dark:border-neutral-600 dark:bg-neutral-800
      "
      aria-label="Table of contents"
    >
      <TocSidebar {toc} onnavigate={close} />
    </nav>
  {/if}
</div>
