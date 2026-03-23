<script lang="ts">
  import { extractDocPath } from "../state/router.svelte";
  import { getRwContext } from "../lib/context";
  import { watchPageSection } from "../lib/sectionWatcher.svelte";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  // Load page when path changes.
  // In embedded mode, wait for scope/basePath resolution — the initial path
  // may not yet include the section prefix, so loading now would fetch the
  // wrong page.
  $effect(() => {
    if (router.embedded && !router.resolved) return;
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
