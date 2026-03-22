<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { watchPageSection } from "../lib/sectionWatcher.svelte";
  import PageContent from "../components/PageContent.svelte";

  const { page, navigation, liveReload } = getRwContext();

  watchPageSection(page, navigation);

  $effect(() => {
    page.load("");
    return liveReload.onReload(() => {
      page.load("", { bypassCache: true });
    });
  });
</script>

<PageContent />
