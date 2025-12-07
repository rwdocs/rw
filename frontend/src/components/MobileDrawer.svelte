<script lang="ts">
  import { mobileMenuOpen, closeMobileMenu } from "../stores/ui";
  import { path } from "../stores/router";
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
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 bg-black/50 z-40 md:hidden"
    onclick={closeMobileMenu}
    role="button"
    tabindex="-1"
    aria-label="Close menu"
  ></div>

  <!-- Drawer -->
  <aside
    class="fixed inset-y-0 left-0 w-[280px] bg-white z-50 shadow-xl overflow-y-auto md:hidden"
  >
    <div class="p-4">
      <div class="flex items-center justify-between mb-6">
        <a href="/" class="block">
          <span class="text-xl font-semibold text-gray-900">Docstage</span>
        </a>
        <button
          onclick={closeMobileMenu}
          class="p-2 -mr-2 text-gray-500 hover:text-gray-700"
          aria-label="Close menu"
        >
          <svg
            class="w-5 h-5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>
      <NavigationSidebar />
    </div>
  </aside>
{/if}
