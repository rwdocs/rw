<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { watchPageScope } from "../lib/scopeWatcher.svelte";
  import PageContent from "../components/PageContent.svelte";

  const { page, navigation, liveReload } = getRwContext();

  watchPageScope(page, navigation);

  $effect(() => {
    page.load("");
    return liveReload.onReload(() => {
      page.load("", { bypassCache: true });
    });
  });
</script>

<PageContent />
