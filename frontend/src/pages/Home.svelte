<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "../stores/page";
  import { liveReload } from "../stores/liveReload";

  // Load root index page
  onMount(() => {
    page.load("");
    return liveReload.onReload(() => {
      page.load("", { bypassCache: true });
    });
  });
</script>

<div class="transition-opacity duration-150 {$page.loading ? 'opacity-0' : 'opacity-100'}">
  {#if $page.notFound}
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
    <article class="prose prose-slate max-w-none">
      {@html $page.data.content}
    </article>
  {/if}
</div>
