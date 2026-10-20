<script lang="ts">
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import CloseIcon from "$lib/ui/primitives/CloseIcon.svelte";
  import { trapFocus } from "$lib/ui/hooks/trapFocus";
  import { onViewerKeydown } from "$lib/ui/viewerKeydown";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  // Not on the panel alone: Escape must still close after a click on a toast,
  // outside the focus trap. Closing unmounts the panel, and trapFocus's cleanup
  // restores focus to the trigger.
  function closeOnEscape(panel: HTMLElement) {
    return onViewerKeydown(panel, (e) => {
      if (e.key === "Escape") onClose();
    });
  }
</script>

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
      {@attach closeOnEscape}
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
              <CloseIcon class="size-5" />
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
