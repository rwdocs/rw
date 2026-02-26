<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getRwContext } from "../lib/context";
  import { watchPageScope } from "../lib/scopeWatcher";
  import PageContent from "../components/PageContent.svelte";

  const { page, navigation, liveReload } = getRwContext();

  const unsubscribePage = watchPageScope(page, navigation);

  onMount(() => {
    page.load("");
    return liveReload.onReload(() => {
      page.load("", { bypassCache: true });
    });
  });

  onDestroy(() => {
    unsubscribePage();
  });
</script>

<PageContent />
