<script lang="ts">
  import { getRwContext } from "$lib/context";
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
      // Silent so a live reload doesn't flip `loading` → show the skeleton (which
      // unmounts the article after 300ms) and lose the reader's scroll position.
      // Mirrors Page.svelte's reload; the initial load above stays non-silent.
      page.load("", { bypassCache: true, silent: true });
    });
  });
</script>

<PageContent />
