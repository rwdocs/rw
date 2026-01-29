<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { page } from "../stores/page";
  import { liveReload } from "../stores/liveReload";
  import { watchPageScope } from "../lib/scopeWatcher";
  import PageContent from "../components/PageContent.svelte";

  const unsubscribePage = watchPageScope(page);

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
