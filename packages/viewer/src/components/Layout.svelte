<script lang="ts">
  import { onMount, untrack } from "svelte";
  import type { Snippet } from "svelte";
  import { get } from "svelte/store";
  import { getRwContext } from "../lib/context";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import TocPopover from "./TocPopover.svelte";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import MobileDrawer from "./MobileDrawer.svelte";
  import LoadingBar from "./LoadingBar.svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  const { router, navigation, page, ui } = getRwContext();
  const routerPath = router.path;
  const homeHref = router.prefixPath("/");

  let contentArea: HTMLElement;

  onMount(async () => {
    await navigation.load();
    // Expand to current path after initial load
    const currentPath = get(router.path);
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }
  });

  // Scroll content area to top when navigating to a new page (without hash)
  $effect(() => {
    void $routerPath;
    if (!untrack(() => get(router.hash)) && contentArea) {
      contentArea.scrollTop = 0;
    }
  });

  // Close TOC popover when navigating to a different page
  $effect(() => {
    void $page.data?.meta.path;
    ui.closeTocPopover();
  });
</script>

<div class="layout-container" data-testid="viewer-root">
  <LoadingBar loading={$page.loading} />
  <!-- Mobile Header -->
  <header
    class="
      layout-mobile-header sticky top-0 z-30 flex items-center border-b border-gray-200 bg-white
      px-4 py-3
      dark:border-neutral-700 dark:bg-neutral-800
    "
  >
    <button
      onclick={ui.openMobileMenu}
      class="
        -ml-2 cursor-pointer p-2 text-gray-500
        hover:text-gray-700
        dark:text-neutral-400
        dark:hover:text-neutral-300
      "
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
        ><span class="text-gray-900 dark:text-neutral-100">R</span><span
          class="text-gray-400 dark:text-neutral-500">W</span
        ></span
      >
    </a>
  </header>

  <!-- Mobile Drawer -->
  <MobileDrawer />
  <div
    class="
      layout-root flex h-full flex-col bg-white text-gray-900
      dark:bg-neutral-800 dark:text-neutral-100
    "
  >
    <!-- Navigation Sidebar (Desktop) -->
    <aside
      aria-label="Sidebar"
      class="
        layout-sidebar hidden h-full w-[280px] shrink-0 overflow-y-auto border-r border-gray-200
        dark:border-neutral-700
      "
    >
      <div class="px-4 pt-6 pb-4">
        <a href={homeHref} class="mb-7 flex min-h-8 items-center pl-[6px]">
          <span class="text-xl font-semibold uppercase"
            ><span class="text-gray-900 dark:text-neutral-100">R</span><span
              class="text-gray-400 dark:text-neutral-500">W</span
            ></span
          >
        </a>
        {#if $navigation.error}
          <div
            class="
              mb-4 rounded-sm border border-red-200 bg-red-50 p-3 text-sm text-red-700
              dark:border-red-800 dark:bg-red-950 dark:text-red-300
            "
          >
            Failed to load navigation: {$navigation.error}
          </div>
        {/if}
        <NavigationSidebar />
      </div>
    </aside>

    <!-- Main Content + ToC Container -->
    <div
      class="min-w-0 flex-1 overflow-y-auto"
      data-testid="content-scroll-area"
      bind:this={contentArea}
    >
      <div class="layout-content mx-auto max-w-6xl px-4 pt-6 pb-12">
        {#if $page.data}
          {#if $page.data.toc.length > 0}
            <div class="layout-toc-popover sticky top-6 z-30 float-right mt-[-6px]">
              <TocPopover toc={$page.data.toc} />
            </div>
          {/if}
          <Breadcrumbs breadcrumbs={$page.data.breadcrumbs} />
        {:else if $page.loading}
          <!-- Reserve breadcrumb space during first load (matches Breadcrumbs nav mb-6 + h-8) -->
          <div class="mb-6 h-8"></div>
        {/if}
        <div class="flex">
          <!-- Main Content -->
          <main class="min-w-0 flex-1">
            {@render children()}
          </main>

          <!-- Table of Contents Sidebar - reserve space during loading for consistent skeleton layout -->
          {#if $page.loading || ($page.data && $page.data.toc.length > 0)}
            <aside aria-label="Page outline" class="layout-toc hidden w-[240px] shrink-0">
              {#if $page.data && $page.data.toc.length > 0}
                <div
                  class="sticky top-6 max-h-[calc(100cqb-1.5rem)] overflow-y-auto pl-8"
                  data-testid="toc-sticky-wrapper"
                >
                  <TocSidebar toc={$page.data.toc} />
                </div>
              {/if}
            </aside>
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  /* Use container queries instead of viewport breakpoints so the layout
     adapts to actual available space (important when embedded in a host app). */
  .layout-container {
    container-type: size;
    position: relative;
    height: 100%;
    overflow: hidden;
  }

  /* 952px = sidebar (280px) + comfortable content area (~672px) */
  @container (min-width: 952px) {
    .layout-mobile-header {
      display: none;
    }
    .layout-root {
      flex-direction: row;
    }
    .layout-sidebar {
      display: block;
    }
    .layout-content {
      padding-left: 2rem;
      padding-right: 2rem;
    }
  }

  /* 1224px = sidebar (280px) + content (~704px) + TOC (240px) */
  @container (min-width: 1224px) {
    .layout-toc {
      display: block;
    }
    .layout-toc-popover {
      display: none;
    }
  }
</style>
