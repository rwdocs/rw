<script lang="ts">
  import { onMount } from "svelte";
  import { extractDocPath } from "../state/router.svelte";
  import { getRwContext } from "../lib/context";
  import { watchPageScope } from "../lib/scopeWatcher.svelte";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  // Load page when path changes
  $effect(() => {
    const currentPath = router.path;
    const apiPath = extractDocPath(currentPath);
    page.load(apiPath);
  });

  watchPageScope(page, navigation);

  onMount(() => {
    return liveReload.onReload(() => {
      page.load(extractDocPath(router.path), { bypassCache: true, silent: true });
    });
  });
</script>

<PageContent />
