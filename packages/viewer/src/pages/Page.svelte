<script lang="ts">
  import { untrack } from "svelte";
  import { extractDocPath } from "../state/router.svelte";
  import { getRwContext } from "$lib/context";
  import PageContent from "../components/PageContent.svelte";

  const { router, page, navigation, liveReload } = getRwContext();

  // In embedded mode, wait for scope/basePath resolution — the initial path
  // may not yet include the section prefix, so loading now would fetch the
  // wrong page.
  $effect(() => {
    if (router.embedded && !router.resolved) return;
    void page.load(extractDocPath(router.path));
  });

  $effect(() => {
    if (!page.data) return;
    const sectionRef = page.data.meta.sectionRef;
    // Untracked: re-run only on a new page, not when loadSection() updates it.
    const current = untrack(() => navigation.currentSectionRef);
    if (sectionRef !== current) {
      void navigation.loadSection(sectionRef);
    }
  });

  $effect(() => {
    return liveReload.onReload(() => {
      // Silent so a reload doesn't flash the skeleton and lose scroll position.
      void page.load(extractDocPath(router.path), { bypassCache: true, silent: true });
    });
  });
</script>

<PageContent />
