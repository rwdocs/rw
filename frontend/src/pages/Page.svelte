<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { path, extractDocPath } from "../stores/router";
  import { page } from "../stores/page";
  import { navigation } from "../stores/navigation";
  import { liveReload } from "../stores/liveReload";
  import { watchPageScope } from "../lib/scopeWatcher";
  import PageContent from "../components/PageContent.svelte";

  // Load page when path changes using store subscription
  const unsubscribePath = path.subscribe((currentPath) => {
    const apiPath = extractDocPath(currentPath);
    page.load(apiPath);
    navigation.expandOnlyTo(currentPath);
  });

  const unsubscribePage = watchPageScope(page);

  onMount(() => {
    return liveReload.onReload(() => {
      page.load(extractDocPath(get(path)), { bypassCache: true, silent: true });
    });
  });

  onDestroy(() => {
    unsubscribePath();
    unsubscribePage();
  });
</script>

<PageContent />
