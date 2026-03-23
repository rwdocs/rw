<script lang="ts">
  import { untrack } from "svelte";
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
    /** Section ref for this entity (e.g. "system:default/payment-gateway"). */
    sectionRef?: string;
    /** Called when the user navigates (embedded mode only). Receives the full href. */
    onNavigate?: (href: string) => void;
    /** Custom fetch function (e.g. Backstage authenticated fetch). */
    fetchFn?: typeof fetch;
    /** Called during mount with the router's goto function, for external navigation control. */
    exposeGoto?: (goto: (path: string) => void) => void;
    /** Resolve section refs to base URLs for cross-entity navigation. */
    resolveSectionRefs?: (refs: string[]) => Promise<Record<string, string>>;
  }

  let {
    apiBaseUrl = "/api",
    embedded = false,
    initialPath,
    sectionRef,
    onNavigate,
    fetchFn,
    exposeGoto,
    resolveSectionRefs,
  }: Props = $props();

  const apiClient = createApiClient(
    untrack(() => apiBaseUrl),
    untrack(() => fetchFn),
  );
  const router = new Router({
    embedded: untrack(() => embedded),
    initialPath: untrack(() => initialPath),
    onNavigate: untrack(() => onNavigate),
  });

  // Expose goto for external navigation control (e.g. browser back/forward)
  untrack(() => exposeGoto)?.(router.goto);
  const page = new PageState(apiClient, { embedded: untrack(() => embedded) });
  const navigation = new Navigation(apiClient);

  // Configure section ref resolution for nav items and breadcrumbs
  const resolverFn = untrack(() => resolveSectionRefs);
  if (resolverFn) {
    navigation.setSectionRefResolver(resolverFn);
    page.setSectionRefResolver(resolverFn);
  }
  const liveReload = new LiveReload({ router });
  const ui = new Ui();

  // Close menus and expand navigation on any path change
  let previousPath = router.path;
  $effect(() => {
    const currentPath = router.path;
    if (currentPath !== previousPath) {
      previousPath = currentPath;
      ui.closeMobileMenu();
      navigation.expandOnlyTo(currentPath);
    }
  });

  const currentSectionRef = untrack(() => sectionRef);

  // Reload navigation tree when file structure changes
  const unsubStructureReload = liveReload.onStructureReload(async () => {
    await navigation.load({
      bypassCache: true,
      sectionRef: currentSectionRef,
    });
    const currentPath = router.path;
    if (currentPath !== "/") {
      navigation.expandOnlyTo(currentPath);
    }
  });

  setRwContext({
    apiClient,
    router,
    page,
    navigation,
    liveReload,
    ui,
    resolveSectionRefs: untrack(() => resolveSectionRefs),
  });

  const defaultConfig: ConfigResponse = {
    liveReloadEnabled: false,
  };

  let rootElement: HTMLElement;

  $effect(() => {
    const cleanupRouter = router.initRouter(rootElement);

    (async () => {
      // Load navigation — pass sectionRef for scoped loading in embedded mode
      await navigation.load(currentSectionRef ? { sectionRef: currentSectionRef } : undefined);

      // Extract scopePath from navigation response (embedded mode with sectionRef)
      if (navigation.tree?.scope?.path) {
        router.setScopePath(navigation.tree.scope.path);
      }

      const currentPath = router.path;
      if (currentPath !== "/") {
        navigation.expandOnlyTo(currentPath);
      }

      // Set own base URL from the resolved nav tree scope (embedded mode).
      // resolveNavTree already resolves the scope ref, so we reuse it here
      // instead of calling the resolver a second time.
      if (currentSectionRef && navigation.tree?.scope?.href) {
        router.setBasePath(navigation.tree.scope.href);
      } else if (embedded) {
        router.setBasePath("");
      }

      // Fetch config and start live reload
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
    })();

    return () => {
      cleanupRouter?.();
      liveReload.stop();
      unsubStructureReload();
    };
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
