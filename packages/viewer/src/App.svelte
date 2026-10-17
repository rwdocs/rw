<script lang="ts">
  import { untrack } from "svelte";
  import { createApiClient } from "./api/client";
  import { createCommentApiClient, type CommentApiClient } from "./api/comments";
  import { Router } from "./state/router.svelte";
  import { Page as PageState } from "./state/page.svelte";
  import { Navigation } from "./state/navigation.svelte";
  import { LiveReload } from "./state/liveReload.svelte";
  import { Ui } from "./state/ui.svelte";
  import { Comments } from "./state/comments.svelte";
  import "./lib/diagram/rwDiagramElement";
  import type { NotifyFn } from "./types/notify";
  import { setRwContext } from "./lib/context";
  import type { ConfigResponse } from "./types";
  import Layout from "./components/Layout.svelte";
  import Page from "./pages/Page.svelte";
  import NotFound from "./pages/NotFound.svelte";

  interface Props {
    /** API base URL. Defaults to "/_api". */
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
    /** Host-supplied notification sink; falls back to the built-in toaster. */
    onNotify?: NotifyFn;
    /** Host-supplied comment client. When present, comments are enabled and all
     *  reads/writes (and optional live refresh) route through it, decoupled from
     *  `apiBaseUrl`. When absent, the viewer builds the default HTTP client. */
    comments?: CommentApiClient;
  }

  let {
    apiBaseUrl = "/_api",
    embedded = false,
    initialPath,
    sectionRef,
    onNavigate,
    fetchFn,
    exposeGoto,
    resolveSectionRefs,
    onNotify,
    // Renamed to avoid shadowing the local `comments` state created below.
    comments: injectedComments,
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
  const sectionRefResolver = untrack(() => resolveSectionRefs);
  const page = new PageState(apiClient, { embedded: router.embedded, sectionRefResolver });
  const navigation = new Navigation(apiClient, { sectionRefResolver });
  const liveReload = new LiveReload({ router });
  const ui = new Ui();
  const notify = untrack(() => onNotify) ?? ui.pushToast;
  const hostComments = untrack(() => injectedComments);
  const comments = new Comments(
    hostComments ??
      createCommentApiClient(
        untrack(() => apiBaseUrl),
        untrack(() => fetchFn),
      ),
    notify,
  );
  // A host-supplied client enables comments outright; otherwise `/config` decides.
  comments.enabled = hostComments != null;

  // `navigation.load` re-expands to the last path given here, so a tree that
  // arrives after the path changed still opens to it.
  $effect(() => {
    const path = router.path;
    untrack(() => {
      ui.closeMobileMenu();
      navigation.expandOnlyTo(path);
    });
  });

  const currentSectionRef = untrack(() => sectionRef);

  // `navigation.load` reports its own failures.
  const unsubStructureReload = liveReload.on("structure", () => {
    void navigation.load(navigation.currentSectionRef ?? currentSectionRef, { bypassCache: true });
  });

  setRwContext({
    router,
    page,
    navigation,
    liveReload,
    ui,
    comments,
    notify,
  });

  const defaultConfig: ConfigResponse = {
    liveReloadEnabled: false,
  };

  let rootElement: HTMLElement;

  $effect(() => {
    const cleanupRouter = router.initRouter(rootElement);

    void (async () => {
      // Load navigation — pass sectionRef for scoped loading in embedded mode
      await navigation.load(currentSectionRef);

      // Extract scopePath from navigation response (embedded mode with sectionRef).
      // Skip root scope ("/") — it has no path prefix to strip/add.
      if (navigation.tree?.scope?.path && navigation.tree.scope.path !== "/") {
        router.setScopePath(navigation.tree.scope.path);
      }

      // Set own base URL from the resolved nav tree scope (embedded mode).
      // resolveNavTree already resolves the scope ref, so we reuse it here
      // instead of calling the resolver a second time.
      if (currentSectionRef && navigation.tree?.scope?.href) {
        router.setBasePath(navigation.tree.scope.href);
      } else if (router.embedded) {
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

      if (config.liveReloadEnabled && !router.embedded) {
        liveReload.connect();
      }

      if (config.commentsEnabled) {
        comments.enabled = true;
      }
    })();

    return () => {
      cleanupRouter?.();
      liveReload.disconnect();
      unsubStructureReload();
    };
  });

  // "/" is a document page too: one Page instance spans root and doc paths,
  // so navigating between them doesn't remount it.
  const getRoute = (currentPath: string) => {
    if (import.meta.env.DEV && currentPath === "/_kit") return "kit";
    // Skip static assets and the reserved /_api/ namespace
    if (currentPath.startsWith("/assets/") || currentPath.startsWith("/_api/")) {
      return "notfound";
    }
    return "page";
  };

  let route = $derived(getRoute(router.path));
</script>

<div bind:this={rootElement} class="h-full">
  <Layout>
    {#if route === "page"}
      <Page />
    {:else if route === "kit"}
      {#await import("./pages/Kit.svelte") then { default: Kit }}
        <Kit />
      {/await}
    {:else}
      <NotFound />
    {/if}
  </Layout>
</div>
