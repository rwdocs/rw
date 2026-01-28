<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "../stores/page";
  import { liveReload } from "../stores/liveReload";
  import PageContent from "../components/PageContent.svelte";

  // Load root index page
  onMount(() => {
    page.load("");
    return liveReload.onReload(() => {
      page.load("", { bypassCache: true });
    });
  });
</script>

<PageContent />
