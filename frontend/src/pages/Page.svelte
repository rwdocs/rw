<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { path } from "../stores/router";
  import { page } from "../stores/page";
  import { liveReload } from "../stores/liveReload";

  // Extract path from location, removing /docs prefix
  let docPath = $derived($path.replace(/^\/docs\/?/, ""));

  // Load page when path changes (track previous to avoid duplicate loads)
  let previousPath: string | null = null;
  $effect(() => {
    if (docPath !== previousPath) {
      previousPath = docPath;
      page.load(docPath);
    }
  });

  onMount(() => {
    return liveReload.onReload(() => {
      const currentDocPath = get(path).replace(/^\/docs\/?/, "");
      page.load(currentDocPath, { bypassCache: true });
    });
  });
</script>

{#if $page.loading}
  <div class="flex items-center justify-center h-64">
    <p class="text-gray-600">Loading...</p>
  </div>
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
  <article class="prose prose-slate max-w-none">
    {@html $page.data.content}
  </article>
{/if}
