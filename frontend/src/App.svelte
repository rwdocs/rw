<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import { createApiClient } from "./api/client";
  import { createRouter } from "./stores/router";
  import { createPageStore } from "./stores/page";
  import { createNavigationStore } from "./stores/navigation";
  import { createLiveReloadStore } from "./stores/liveReload";
  import { setRwContext } from "./lib/context";
  import type { ConfigResponse } from "./types";
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

  const apiClient = createApiClient(untrack(() => apiBaseUrl));
  const router = createRouter({
    embedded: untrack(() => embedded),
    initialPath: untrack(() => initialPath),
  });
  const page = createPageStore(apiClient, { embedded: untrack(() => embedded) });
  const navigation = createNavigationStore(apiClient);
  const liveReload = createLiveReloadStore({ router, navigation });

  setRwContext({ apiClient, router, page, navigation, liveReload });

  const defaultConfig: ConfigResponse = {
    liveReloadEnabled: false,
  };

  let rootElement: HTMLElement;
  let cleanupRouter: (() => void) | undefined;

  onMount(async () => {
    cleanupRouter = router.initRouter(rootElement);

    let config = defaultConfig;
    try {
      config = await apiClient.fetchConfig();
    } catch (e) {
      if (import.meta.env.DEV) {
        console.warn("[App] Failed to fetch config, using defaults:", e);
      }
    }

    if (config.liveReloadEnabled && !embedded) {
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

  const { path } = router;
  let route = $derived(getRoute($path));
</script>

<div bind:this={rootElement}>
  <Layout>
    {#if route === "home"}
      <Home />
    {:else if route === "page"}
      <Page />
    {:else}
      <NotFound />
    {/if}
  </Layout>
</div>
