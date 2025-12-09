<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { path, extractDocPath } from "../stores/router";
  import { page } from "../stores/page";
  import { navigation } from "../stores/navigation";
  import { liveReload } from "../stores/liveReload";

  // Extract API path from URL (strips leading slash for API call)
  let apiPath = $derived(extractDocPath($path));

  // Load page when path changes (track previous to avoid duplicate loads)
  let previousPath: string | null = null;
  $effect(() => {
    if (apiPath !== previousPath) {
      previousPath = apiPath;
      page.load(apiPath);
      // Expand only the path to the current page in navigation
      navigation.expandOnlyTo($path);
    }
  });

  onMount(() => {
    return liveReload.onReload(() => {
      page.load(extractDocPath(get(path)), { bypassCache: true });
    });
  });
</script>

<div
  class="transition-opacity duration-150 {$page.loading
    ? 'opacity-0'
    : 'opacity-100'}"
>
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
