<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { watchPageSection } from "../state/sectionWatcher.svelte";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  watchPageSection(page, navigation);

  $effect(() => {
    // In embedded mode, wait for scope/basePath resolution before loading.
    // The path may change once the section scope is known (e.g., "/" → "/domains/billing"),
    // which would unmount Home and mount Page instead — avoid a wasted fetch.
    if (router.embedded && !router.resolved) return;
    page.load("");
    return liveReload.onReload(() => {
      page.load("", { bypassCache: true });
    });
  });
</script>

<PageContent />
