<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { path, initRouter } from "./stores/router";
  import { liveReload } from "./stores/liveReload";
  import { fetchConfig } from "./api/client";
  import Layout from "./components/Layout.svelte";
  import Home from "./pages/Home.svelte";
  import Page from "./pages/Page.svelte";
  import NotFound from "./pages/NotFound.svelte";

  onMount(async () => {
    initRouter();
    try {
      const config = await fetchConfig();
      if (config.liveReloadEnabled) {
        liveReload.start();
      }
    } catch (e) {
      // Live reload is optional - fail silently in production
      if (import.meta.env.DEV) {
        console.warn("[App] Failed to fetch config, live reload disabled:", e);
      }
    }
  });

  onDestroy(() => {
    liveReload.stop();
  });

  // Determine which page to render based on path
  // Any non-root path is treated as a document page
  const getRoute = (currentPath: string) => {
    if (currentPath === "/") return "home";
    // Skip API routes and static assets
    if (currentPath.startsWith("/api/") || currentPath.startsWith("/assets/")) {
      return "notfound";
    }
    return "page";
  };

  let route = $derived(getRoute($path));
</script>

<Layout>
  {#if route === "home"}
    <Home />
  {:else if route === "page"}
    <Page />
  {:else}
    <NotFound />
  {/if}
</Layout>
