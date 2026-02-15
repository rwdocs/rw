<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { navigation } from "../stores/navigation";
  import { openMobileMenu } from "../stores/ui";
  import { page } from "../stores/page";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import MobileDrawer from "./MobileDrawer.svelte";
  import LoadingBar from "./LoadingBar.svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  onMount(async () => {
    await navigation.load();
    // Expand to current path after initial load
    const currentPath = window.location.pathname;
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }
  });
</script>

<LoadingBar loading={$page.loading} />

<!-- Mobile Header -->
<header
  class="sticky top-0 z-30 bg-white border-b border-gray-200 px-4 py-3 flex items-center md:hidden"
>
  <button
    onclick={openMobileMenu}
    class="p-2 -ml-2 text-gray-500 hover:text-gray-700 cursor-pointer"
    aria-label="Open menu"
  >
    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M4 6h16M4 12h16M4 18h16"
      />
    </svg>
  </button>
  <a href="/" class="ml-3">
    <span class="text-lg font-semibold uppercase"
      ><span class="text-gray-900">R</span><span class="text-gray-400">W</span></span
    >
  </a>
</header>

<!-- Mobile Drawer -->
<MobileDrawer />

<div class="min-h-screen flex flex-col md:flex-row">
  <!-- Navigation Sidebar (Desktop) -->
  <aside
    class="w-[280px] flex-shrink-0 border-r border-gray-200 hidden md:block h-screen sticky top-0 overflow-y-auto"
  >
    <div class="pt-6 px-4 pb-4">
      <a href="/" class="block mb-5 pl-[6px]">
        <span class="text-xl font-semibold uppercase"
          ><span class="text-gray-900">R</span><span class="text-gray-400">W</span></span
        >
      </a>
      {#if $navigation.error}
        <div class="p-3 mb-4 text-sm text-red-700 bg-red-50 border border-red-200 rounded">
          Failed to load navigation: {$navigation.error}
        </div>
      {/if}
      <NavigationSidebar />
    </div>
  </aside>

  <!-- Main Content + ToC Container -->
  <div class="flex-1">
    <div class="max-w-6xl mx-auto px-4 md:px-8 pt-6 pb-12">
      {#if $page.data}
        <Breadcrumbs breadcrumbs={$page.data.breadcrumbs} />
      {:else if $page.loading}
        <!-- Reserve breadcrumb space during first load (matches Breadcrumbs mb-6) -->
        <div class="mb-6"></div>
      {/if}
      <div class="flex">
        <!-- Main Content -->
        <main class="flex-1 min-w-0">
          {@render children()}
        </main>

        <!-- Table of Contents Sidebar - reserve space during loading for consistent skeleton layout -->
        {#if $page.loading || ($page.data && $page.data.toc.length > 0)}
          <aside class="w-[240px] flex-shrink-0 hidden lg:block">
            {#if $page.data && $page.data.toc.length > 0}
              <div class="pl-8 sticky top-6 overflow-y-auto max-h-[calc(100vh-1.5rem)]">
                <TocSidebar toc={$page.data.toc} />
              </div>
            {/if}
          </aside>
        {/if}
      </div>
    </div>
  </div>
</div>
