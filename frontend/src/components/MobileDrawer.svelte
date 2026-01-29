<script lang="ts">
  import { mobileMenuOpen, closeMobileMenu } from "../stores/ui";
  import { path } from "../stores/router";
  import { navigation } from "../stores/navigation";
  import NavigationSidebar from "./NavigationSidebar.svelte";

  // Close drawer on route change
  $effect(() => {
    void $path;
    closeMobileMenu();
  });

  // Close drawer on escape key
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && $mobileMenuOpen) {
      closeMobileMenu();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $mobileMenuOpen}
  <!-- Backdrop -->
  <button
    type="button"
    class="fixed inset-0 bg-black/50 z-40 md:hidden border-none cursor-pointer"
    onclick={closeMobileMenu}
    aria-label="Close menu"
  ></button>

  <!-- Drawer -->
  <aside class="fixed inset-y-0 left-0 w-[280px] bg-white z-50 shadow-xl overflow-y-auto md:hidden">
    <div class="p-4">
      <div class="flex items-center justify-between mb-6">
        <a href="/" class="block">
          <span class="text-xl font-semibold text-gray-900">RW</span>
        </a>
        <button
          onclick={closeMobileMenu}
          class="p-2 -mr-2 text-gray-500 hover:text-gray-700 cursor-pointer"
          aria-label="Close menu"
        >
          <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <!-- svelte-ignore component_name_lowercase -->
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>
      {#if $navigation.error}
        <div class="p-3 mb-4 text-sm text-red-700 bg-red-50 border border-red-200 rounded">
          Failed to load navigation: {$navigation.error}
        </div>
      {/if}
      <NavigationSidebar />
    </div>
  </aside>
{/if}
