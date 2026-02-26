<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { extractDocPath } from "../stores/router";
  import { getRwContext } from "../lib/context";
  import { watchPageScope } from "../lib/scopeWatcher";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  // Load page when path changes using store subscription
  const unsubscribePath = router.path.subscribe((currentPath) => {
    const apiPath = extractDocPath(currentPath);
    page.load(apiPath);
    navigation.expandOnlyTo(currentPath);
  });

  const unsubscribePage = watchPageScope(page, navigation);

  onMount(() => {
    return liveReload.onReload(() => {
      page.load(extractDocPath(get(router.path)), { bypassCache: true, silent: true });
    });
  });

  onDestroy(() => {
    unsubscribePath();
    unsubscribePage();
  });
</script>

<PageContent />
