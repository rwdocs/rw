<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { path, extractDocPath } from "../stores/router";
  import { page } from "../stores/page";
  import { navigation } from "../stores/navigation";
  import { liveReload } from "../stores/liveReload";
  import PageContent from "../components/PageContent.svelte";

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

<PageContent />
