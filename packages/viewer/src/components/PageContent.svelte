<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { initializeTabs } from "../lib/tabs";
  import { LOADING_SHOW_DELAY } from "../lib/constants";
  import LoadingSkeleton from "./LoadingSkeleton.svelte";

  const { page, router } = getRwContext();
  const { hash } = router;

  let articleRef: HTMLElement | undefined = $state();
  let showSkeleton = $state(false);

  // Show skeleton only if loading takes longer than SHOW_DELAY
  $effect(() => {
    if ($page.loading) {
      const timeout = setTimeout(() => {
        showSkeleton = true;
      }, LOADING_SHOW_DELAY);
      return () => clearTimeout(timeout);
    } else {
      showSkeleton = false;
    }
  });

  // Initialize tabs when content changes
  $effect(() => {
    if ($page.data && articleRef) {
      return initializeTabs(articleRef);
    }
  });

  // Scroll to hash target when content loads or hash changes (skip in embedded mode
  // to avoid scrolling the host page)
  $effect(() => {
    const currentHash = $hash;
    if (!router.embedded && $page.data && articleRef && currentHash) {
      const target = document.getElementById(currentHash);
      if (target) {
        // Use requestAnimationFrame to ensure DOM is fully rendered
        requestAnimationFrame(() => {
          target.scrollIntoView({ behavior: "auto" });
        });
      }
    }
  });
</script>

{#if $page.loading && showSkeleton}
  <LoadingSkeleton />
{:else if $page.loading && $page.data}
  <!-- Fast load: show previous content with reduced opacity -->
  <article
    class="prose max-w-none opacity-50 transition-opacity duration-150 prose-slate dark:prose-invert"
  >
    {@html $page.data.content}
  </article>
{:else if $page.notFound}
  <div class="flex h-64 items-center justify-center">
    <div class="text-center">
      <h1 class="mb-4 text-4xl font-bold tracking-tight text-gray-300 dark:text-neutral-600">
        404
      </h1>
      <p class="text-gray-600 dark:text-neutral-400">Page not found</p>
    </div>
  </div>
{:else if $page.error}
  <div class="flex h-64 items-center justify-center">
    <p class="text-red-600 dark:text-red-400">Error: {$page.error}</p>
  </div>
{:else if $page.data}
  <article bind:this={articleRef} class="prose max-w-none prose-slate dark:prose-invert">
    {@html $page.data.content}
  </article>
{/if}
