<script lang="ts">
  import { extractDocPath } from "../state/router.svelte";
  import { getRwContext } from "$lib/context";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  // In embedded mode, wait for scope/basePath resolution — the initial path
  // may not yet include the section prefix, so loading now would fetch the
  // wrong page.
  $effect(() => {
    if (!router.resolved) return;
    void page.load(extractDocPath(router.path));
  });

  $effect(() => {
    if (!page.data) return;
    void navigation.loadSection(page.data.meta.sectionRef);
  });

  $effect(() => {
    return liveReload.on("content", () => {
      // Silent so a reload doesn't flash the skeleton and lose scroll position.
      void page.load(extractDocPath(router.path), { bypassCache: true, silent: true });
    });
  });
</script>

<PageContent />
