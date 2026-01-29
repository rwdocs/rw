<script lang="ts">
  import { page } from "../stores/page";
  import { hash } from "../stores/router";
  import { initializeTabs } from "../lib/tabs";
  import LoadingSkeleton from "./LoadingSkeleton.svelte";

  // Delay before showing skeleton (ms) - prevents "blink" on fast loads
  const SHOW_DELAY = 150;

  let articleRef: HTMLElement | undefined = $state();
  let showSkeleton = $state(false);

  // Show skeleton only if loading takes longer than SHOW_DELAY
  $effect(() => {
    if ($page.loading) {
      const timeout = setTimeout(() => {
        showSkeleton = true;
      }, SHOW_DELAY);
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

  // Scroll to hash target when content loads or hash changes
  $effect(() => {
    if ($page.data && articleRef && $hash) {
      const target = document.getElementById($hash);
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
{:else if $page.notFound}
  <div class="flex items-center justify-center h-64">
    <div class="text-center">
      <h1 class="text-4xl font-bold tracking-tight text-gray-300 mb-4">404</h1>
      <p class="text-gray-600">Page not found</p>
    </div>
  </div>
{:else if $page.error}
  <div class="flex items-center justify-center h-64">
    <p class="text-red-600">Error: {$page.error}</p>
  </div>
{:else if $page.data}
  <article bind:this={articleRef} class="prose prose-slate max-w-none">
    {@html $page.data.content}
  </article>
{/if}
