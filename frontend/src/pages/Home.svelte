<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { page } from "../stores/page";
  import { navigation } from "../stores/navigation";
  import { liveReload } from "../stores/liveReload";
  import PageContent from "../components/PageContent.svelte";

  // Watch for page scope changes and reload navigation if needed
  const unsubscribePage = page.subscribe((state) => {
    if (state.data) {
      const pageScope = state.data.meta.navigationScope;
      const currentScope = get(navigation).currentScope;
      if (pageScope !== currentScope) {
        navigation.loadScope(pageScope);
      }
    }
  });

  // Load root index page
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
