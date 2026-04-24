<script lang="ts">
  import { getRwContext } from "../lib/context";
  import Popover from "../lib/ui/primitives/Popover.svelte";
  import IconButton from "./IconButton.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
    activeId: string | null;
    onNavigate: (id: string) => void;
  }

  let { toc, activeId, onNavigate }: Props = $props();

  const { router } = getRwContext();

  let open = $state(false);

  // Close on route change so navigating via a TOC entry dismisses the popover.
  // Reading router.path registers the reactive dependency; the body only flips
  // `open` so the Popover's bindable pulls the value back into sync.
  $effect(() => {
    void router.path;
    open = false;
  });
</script>

<Popover bind:open dismissible placement="bottom" align="end" offset={8}>
  {#snippet trigger({ controlProps })}
    <IconButton
      onclick={() => (open = !open)}
      aria-label="Table of contents"
      active={open}
      {...controlProps}
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
    </IconButton>
  {/snippet}
  <nav
    aria-label="Table of contents"
    class="
      max-h-[min(24rem,calc(100dvh-5rem))] w-64 overflow-y-auto rounded-lg border border-gray-200
      bg-white p-4 shadow-lg
      dark:border-neutral-600 dark:bg-neutral-800
    "
  >
    <TocSidebar
      {toc}
      {activeId}
      onNavigate={(id) => {
        onNavigate(id);
        open = false;
      }}
    />
  </nav>
</Popover>
