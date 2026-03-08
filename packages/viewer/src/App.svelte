<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import { createApiClient } from "./api/client";
  import { Router } from "./state/router.svelte";
  import { Page as PageState } from "./state/page.svelte";
  import { Navigation } from "./state/navigation.svelte";
  import { LiveReload } from "./state/liveReload.svelte";
  import { Ui } from "./state/ui.svelte";
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
    /** Path prefix for link hrefs (e.g. "/rw-docs"). */
    basePath?: string;
    /** Called when the user navigates to a new path (embedded mode only). */
    onNavigate?: (path: string) => void;
    /** Custom fetch function (e.g. Backstage authenticated fetch). */
    fetchFn?: typeof fetch;
    /** Called during mount with the router's goto function, for external navigation control. */
    exposeGoto?: (goto: (path: string) => void) => void;
  }

  let {
    apiBaseUrl = "/api",
    embedded = false,
    initialPath,
    basePath,
    onNavigate,
    fetchFn,
    exposeGoto,
  }: Props = $props();

  const apiClient = createApiClient(
    untrack(() => apiBaseUrl),
    untrack(() => fetchFn),
  );
  const router = new Router({
    embedded: untrack(() => embedded),
    initialPath: untrack(() => initialPath),
    basePath: untrack(() => basePath),
    onNavigate: untrack(() => onNavigate),
  });

  // Expose goto for external navigation control (e.g. browser back/forward)
  untrack(() => exposeGoto)?.(router.goto);
  const page = new PageState(apiClient, { embedded: untrack(() => embedded) });
  const navigation = new Navigation(apiClient);
  const liveReload = new LiveReload({ router });
  const ui = new Ui();

  // Close menus and expand navigation on any path change
  let previousPath = router.path;
  $effect(() => {
    const currentPath = router.path;
    if (currentPath !== previousPath) {
      previousPath = currentPath;
      ui.closeMobileMenu();
      ui.closeTocPopover();
      navigation.expandOnlyTo(currentPath);
    }
  });

  // Reload navigation tree when file structure changes
  const unsubStructureReload = liveReload.onStructureReload(async () => {
    await navigation.load({ bypassCache: true });
    const currentPath = router.path;
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }
  });

  setRwContext({ apiClient, router, page, navigation, liveReload, ui });

  const defaultConfig: ConfigResponse = {
    liveReloadEnabled: false,
  };

  let rootElement: HTMLElement;
  let cleanupRouter: (() => void) | undefined;

  onMount(async () => {
    cleanupRouter = router.initRouter(rootElement);

    // Load navigation tree and expand to current path
    await navigation.load();
    const currentPath = router.path;
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }

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
    unsubStructureReload();
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

  let route = $derived(getRoute(router.path));
</script>

<div bind:this={rootElement} class="h-full">
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
