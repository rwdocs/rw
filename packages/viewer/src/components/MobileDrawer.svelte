<script lang="ts">
  import { getRwContext } from "$lib/context";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import { trapFocus } from "$lib/ui/hooks/trapFocus";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  const { router } = getRwContext();

  // Window-level so Escape still closes after a click on a toast (outside the
  // focus trap). Skipped in embedded mode to avoid interfering with the host app
  // — there the Close button and backdrop dismiss. Closing unmounts the panel,
  // and trapFocus's cleanup restores focus to the trigger.
  function onKeydown(e: KeyboardEvent) {
    if (open && !router.embedded && e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div class="drawer-flow-anchor z-dropdown">
    <button type="button" class="drawer-flow-backdrop" onclick={onClose} aria-label="Close menu"
    ></button>
    <div
      aria-label="Mobile navigation"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      class="drawer-flow-panel"
      {@attach trapFocus}
    >
      <div data-testid="mobile-drawer-panel" class="h-dvh overflow-y-auto bg-bg-raised shadow-xl">
        <div class="p-4">
          <div class="mb-6 flex justify-end">
            <Button
              variant="ghost"
              iconOnly
              onclick={onClose}
              class="-mr-2"
              aria-label="Close menu"
            >
              <svg class="size-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </Button>
          </div>
          <NavigationSidebar />
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  /* Sticky wrapper keeps drawer viewport-aligned while staying within the
     container's horizontal bounds.  Height is 0 so it doesn't affect layout. */
  .drawer-flow-anchor {
    position: sticky;
    top: 0;
    height: 0;
  }

  .drawer-flow-backdrop {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100dvh;
    cursor: pointer;
    border: none;
    background: rgb(0 0 0 / 0.5);
  }

  .drawer-flow-panel {
    position: absolute;
    top: 0;
    left: 0;
    width: 280px;
    height: 100dvh;
  }
</style>
