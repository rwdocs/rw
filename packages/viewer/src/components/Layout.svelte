<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { get } from "svelte/store";
  import { getRwContext } from "../lib/context";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import MobileDrawer from "./MobileDrawer.svelte";
  import LoadingBar from "./LoadingBar.svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  const { router, navigation, page, ui } = getRwContext();
  const homeHref = router.prefixPath("/");

  onMount(async () => {
    await navigation.load();
    // Expand to current path after initial load
    const currentPath = get(router.path);
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }
  });
</script>

<LoadingBar loading={$page.loading} />

<!-- Mobile Header -->
<header
  class="sticky top-0 z-30 flex items-center border-b border-gray-200 bg-white px-4 py-3 md:hidden"
>
  <button
    onclick={ui.openMobileMenu}
    class="-ml-2 cursor-pointer p-2 text-gray-500 hover:text-gray-700"
    aria-label="Open menu"
  >
    <svg class="size-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M4 6h16M4 12h16M4 18h16"
      />
    </svg>
  </button>
  <a href={homeHref} class="ml-3">
    <span class="text-lg font-semibold uppercase"
      ><span class="text-gray-900">R</span><span class="text-gray-400">W</span></span
    >
  </a>
</header>

<!-- Mobile Drawer -->
<MobileDrawer />

<div class="flex min-h-screen flex-col md:flex-row">
  <!-- Navigation Sidebar (Desktop) -->
  <aside
    class="
      sticky top-0 hidden h-screen w-[280px] shrink-0 overflow-y-auto border-r border-gray-200
      md:block
    "
  >
    <div class="px-4 pt-6 pb-4">
      <a href={homeHref} class="mb-5 block pl-[6px]">
        <span class="text-xl font-semibold uppercase"
          ><span class="text-gray-900">R</span><span class="text-gray-400">W</span></span
        >
      </a>
      {#if $navigation.error}
        <div class="mb-4 rounded-sm border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          Failed to load navigation: {$navigation.error}
        </div>
      {/if}
      <NavigationSidebar />
    </div>
  </aside>

  <!-- Main Content + ToC Container -->
  <div class="flex-1">
    <div class="mx-auto max-w-6xl px-4 pt-6 pb-12 md:px-8">
      {#if $page.data}
        <Breadcrumbs breadcrumbs={$page.data.breadcrumbs} />
      {:else if $page.loading}
        <!-- Reserve breadcrumb space during first load (matches Breadcrumbs mb-6) -->
        <div class="mb-6"></div>
      {/if}
      <div class="flex">
        <!-- Main Content -->
        <main class="min-w-0 flex-1">
          {@render children()}
        </main>

        <!-- Table of Contents Sidebar - reserve space during loading for consistent skeleton layout -->
        {#if $page.loading || ($page.data && $page.data.toc.length > 0)}
          <aside class="hidden w-[240px] shrink-0 lg:block">
            {#if $page.data && $page.data.toc.length > 0}
              <div class="sticky top-6 max-h-[calc(100vh-1.5rem)] overflow-y-auto pl-8">
                <TocSidebar toc={$page.data.toc} />
              </div>
            {/if}
          </aside>
        {/if}
      </div>
    </div>
  </div>
</div>
