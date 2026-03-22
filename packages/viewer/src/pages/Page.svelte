<script lang="ts">
  import { extractDocPath } from "../state/router.svelte";
  import { getRwContext } from "../lib/context";
  import { watchPageSection } from "../lib/sectionWatcher.svelte";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  // Load page when path changes
  $effect(() => {
    const currentPath = router.path;
    const apiPath = extractDocPath(currentPath);
    page.load(apiPath);
  });

  watchPageSection(page, navigation);

  $effect(() => {
    return liveReload.onReload(() => {
      page.load(extractDocPath(router.path), { bypassCache: true, silent: true });
    });
  });
</script>

<PageContent />
