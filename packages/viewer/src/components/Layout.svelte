<script lang="ts">
  import { untrack } from "svelte";
  import type { Snippet } from "svelte";
  import { getRwContext } from "../lib/context";
  import NavigationSidebar from "./NavigationSidebar.svelte";
  import TocSidebar from "./TocSidebar.svelte";
  import TocPopover from "./TocPopover.svelte";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import MobileDrawer from "./MobileDrawer.svelte";
  import IconButton from "./IconButton.svelte";
  import LoadingBar from "./LoadingBar.svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  const { router, navigation, page, ui } = getRwContext();
  const homeHref = router.prefixPath("/");

  // Scroll to top when navigating to a new page (without hash)
  $effect(() => {
    void router.path;
    if (!untrack(() => router.hash)) {
      window.scrollTo(0, 0);
    }
  });
</script>

<div class="layout-container" data-testid="viewer-root">
  <LoadingBar loading={page.loading} />
  <!-- Mobile Drawer (before header so the sticky anchor covers it in flow mode) -->
  <MobileDrawer />

  <!-- Mobile Header -->
  <header
    aria-label="Mobile header"
    class="
      layout-mobile-header sticky top-0 z-30 flex items-center border-b border-gray-200 bg-white
      px-4 py-2
      dark:border-neutral-700 dark:bg-neutral-800
    "
  >
    <IconButton onclick={ui.openMobileMenu} aria-label="Open menu" class="mr-2 shrink-0">
      <svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
        <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16" />
      </svg>
    </IconButton>
    {#if page.data}
      <div class="min-w-0 flex-1">
        <Breadcrumbs breadcrumbs={page.data.breadcrumbs} compact />
      </div>
      {#if page.data.toc.length > 0}
        <div class="ml-2 shrink-0">
          <TocPopover toc={page.data.toc} />
        </div>
      {/if}
    {/if}
  </header>
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
        {#if navigation.error}
          <div
            class="
              mb-4 rounded-sm border border-red-200 bg-red-50 p-3 text-sm text-red-700
              dark:border-red-800 dark:bg-red-950 dark:text-red-300
            "
          >
            Failed to load navigation: {navigation.error}
          </div>
        {/if}
        <NavigationSidebar />
      </div>
    </aside>

    <!-- Main Content + ToC Container -->
    <div class="layout-content-area min-w-0" data-testid="content-area">
      <div class="layout-content mx-auto max-w-6xl px-4 pt-6 pb-12">
        {#if page.data}
          {#if page.data.toc.length > 0}
            <div class="layout-toc-popover sticky top-6 z-30 float-right">
              <TocPopover toc={page.data.toc} />
            </div>
          {/if}
          <div class="layout-desktop-breadcrumbs">
            <Breadcrumbs breadcrumbs={page.data.breadcrumbs} />
          </div>
        {:else if page.loading}
          <!-- Reserve breadcrumb space during first load (matches Breadcrumbs nav mb-6 + h-8) -->
          <div class="mb-6 h-8"></div>
        {/if}
        <div class="flex">
          <!-- Main Content -->
          <main class="min-w-0 flex-1">
            {@render children()}
          </main>

          <!-- Table of Contents Sidebar -->
          {#if page.data && page.data.toc.length > 0}
            <aside aria-label="Page outline" class="layout-toc hidden w-[240px] shrink-0">
              {#if page.data && page.data.toc.length > 0}
                <div
                  class="layout-toc-sticky sticky top-6 overflow-y-auto pl-8"
                  data-testid="toc-sticky-wrapper"
                >
                  <TocSidebar toc={page.data.toc} />
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
    container-type: inline-size;
    position: relative;
    height: auto;
    overflow-x: clip;
    overflow-y: visible;
  }

  .layout-root {
    height: auto;
    /* The window scrolls, so headings need enough scroll-margin
       to clear the sticky mobile header (~49px).  Set on .layout-root (not
       .layout-container) so the @container query at 952px can reset it —
       container queries cannot target the container element itself. */
    --scroll-anchor-offset: 3.5rem;
  }

  .layout-content-area {
    overflow-y: visible;
    flex: 0 1 auto;
  }

  .layout-sidebar {
    position: sticky;
    top: 0;
    height: 100vh;
  }

  .layout-toc-sticky {
    max-height: calc(100vh - 1.5rem);
  }

  /* Hide desktop breadcrumbs and content TOC popover on mobile —
     they live in the mobile header instead. */
  .layout-desktop-breadcrumbs {
    display: none;
  }
  .layout-toc-popover {
    display: none;
  }

  /* Wider padding once there's enough horizontal space */
  @container (min-width: 640px) {
    .layout-mobile-header {
      padding-left: 2rem;
      padding-right: 2rem;
    }
    .layout-content {
      padding-left: 2rem;
      padding-right: 2rem;
    }
  }

  /* 952px = sidebar (280px) + comfortable content area (~672px) */
  @container (min-width: 952px) {
    .layout-mobile-header {
      display: none;
    }
    .layout-root {
      flex-direction: row;
      --scroll-anchor-offset: 1.5rem;
    }
    .layout-content-area {
      flex-grow: 1;
    }
    .layout-sidebar {
      display: block;
    }
    .layout-desktop-breadcrumbs {
      display: block;
    }
    .layout-toc-popover {
      display: block;
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
