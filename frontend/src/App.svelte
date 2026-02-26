<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { path, initRouter, setEmbedded, goto } from "./stores/router";
  import { liveReload } from "./stores/liveReload";
  import type { ConfigResponse } from "./types";
  import { setApiBase, fetchConfig } from "./api/client";
  import Layout from "./components/Layout.svelte";
  import Home from "./pages/Home.svelte";
  import Page from "./pages/Page.svelte";
  import NotFound from "./pages/NotFound.svelte";

  interface Props {
    /** API base URL. Defaults to "/api". */
    apiBaseUrl?: string;
    /** Run in embedded mode (no pushState). Defaults to false. */
    embedded?: boolean;
    /** Initial path to navigate to. Defaults to current window path. */
    initialPath?: string;
  }

  let { apiBaseUrl = "/api", embedded = false, initialPath }: Props = $props();

  const defaultConfig: ConfigResponse = {
    liveReloadEnabled: false,
  };

  let cleanupRouter: (() => void) | undefined;

  onMount(async () => {
    setApiBase(apiBaseUrl);
    setEmbedded(embedded);

    if (embedded && initialPath) {
      goto(initialPath);
    }

    cleanupRouter = initRouter();

    let config = defaultConfig;
    try {
      config = await fetchConfig();
    } catch (e) {
      if (import.meta.env.DEV) {
        console.warn("[App] Failed to fetch config, using defaults:", e);
      }
    }

    if (config.liveReloadEnabled) {
      liveReload.start();
    }
  });

  onDestroy(() => {
    cleanupRouter?.();
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
