<script lang="ts">
  import { getRwContext } from "$lib/context";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import { dismissible } from "$lib/ui/hooks/dismissible";

  interface Props {
    open: boolean;
    onClose: () => void;
    error?: string | null;
  }

  let { open, onClose, error = null }: Props = $props();

  const { router } = getRwContext();

  let drawerEl: HTMLElement | undefined = $state();

  // Skip Escape in embedded mode to avoid interfering with host app.
  $effect(() => {
    if (router.embedded) return;
    return dismissible(open, drawerEl, onClose);
  });
</script>

{#if open}
  <div bind:this={drawerEl} class="drawer-flow-anchor">
    <button type="button" class="drawer-flow-backdrop" onclick={onClose} aria-label="Close menu"
    ></button>
    <aside aria-label="Mobile navigation" class="drawer-flow-panel">
      <div
        data-testid="mobile-drawer-panel"
        class="h-dvh overflow-y-auto bg-white shadow-xl dark:bg-neutral-800"
      >
        <div class="p-4">
          <div class="mb-6 flex items-center justify-between">
            <a href={router.prefixPath("/")} class="block">
              <span class="text-xl font-semibold text-gray-900 dark:text-neutral-100">RW</span>
            </a>
            <Button
              variant="ghost"
              iconOnly
              onclick={onClose}
              class="-mr-2"
              aria-label="Close menu"
            >
              <svg class="size-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <!-- svelte-ignore component_name_lowercase -->
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </Button>
          </div>
          {#if error}
            <Alert intent="danger" class="mb-4">
              Failed to load navigation: {error}
            </Alert>
          {/if}
          <NavigationSidebar />
        </div>
      </div>
    </aside>
  </div>
{/if}

<style>
  /* Sticky wrapper keeps drawer viewport-aligned while staying within the
     container's horizontal bounds.  Height is 0 so it doesn't affect layout. */
  .drawer-flow-anchor {
    position: sticky;
    top: 0;
    height: 0;
    z-index: 40;
  }

  .drawer-flow-backdrop {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100dvh;
    z-index: 40;
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
    z-index: 50;
  }
</style>
