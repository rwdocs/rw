<script lang="ts">
  import { onMount } from "svelte";
  import { navigation } from "../stores/navigation";
  import { openMobileMenu } from "../stores/ui";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import MobileDrawer from "./MobileDrawer.svelte";
  import { page } from "../stores/page";
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  onMount(() => {
    navigation.load();
  });
</script>

<!-- Mobile Header -->
<header
  class="sticky top-0 z-30 bg-white border-b border-gray-200 px-4 py-3 flex items-center md:hidden"
>
  <button
    onclick={openMobileMenu}
    class="p-2 -ml-2 text-gray-500 hover:text-gray-700"
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
    <span class="text-lg font-semibold text-gray-900">Docstage</span>
  </a>
</header>

<!-- Mobile Drawer -->
<MobileDrawer />

<div class="h-full flex md:h-screen">
  <!-- Navigation Sidebar (Desktop) -->
  <aside
    class="w-[280px] flex-shrink-0 border-r border-gray-200 overflow-y-auto hidden md:block"
  >
    <div class="p-4">
      <a href="/" class="block mb-6">
        <span class="text-xl font-semibold text-gray-900">Docstage</span>
      </a>
      <NavigationSidebar />
    </div>
  </aside>

  <!-- Main Content -->
  <main class="flex-1 overflow-y-auto">
    <div class="max-w-4xl mx-auto px-4 md:px-8 py-6">
      {#if $page.data}
        <Breadcrumbs breadcrumbs={$page.data.breadcrumbs} />
      {/if}
      {@render children()}
    </div>
  </main>

  <!-- Table of Contents Sidebar -->
  {#if $page.data && $page.data.toc.length > 0}
    <aside
      class="w-[240px] flex-shrink-0 border-l border-gray-200 overflow-y-auto hidden lg:block"
    >
      <div class="p-4 sticky top-0">
        <TocSidebar toc={$page.data.toc} />
      </div>
    </aside>
  {/if}
</div>
