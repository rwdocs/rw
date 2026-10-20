<script lang="ts">
  import { untrack } from "svelte";
  import { createApiClient } from "./api/client";
  import { createCommentApiClient, type CommentApiClient } from "./api/comments";
  import { Router } from "./state/router.svelte";
  import { Page as PageState } from "./state/page.svelte";
  import { Navigation } from "./state/navigation.svelte";
  import { LiveReload } from "./state/liveReload";
  import { Ui } from "./state/ui.svelte";
  import { Comments } from "./state/comments.svelte";
  import "./lib/diagram/rwDiagramElement";
  import type { NotifyFn } from "./types/notify";
  import { setRwContext } from "./lib/context";
  import type { ConfigResponse } from "./types";
  import Layout from "./components/Layout.svelte";
  import Page from "./pages/Page.svelte";

  interface Props {
    /** API base URL. Defaults to "/_api". */
    apiBaseUrl?: string;
    /** Mounted by `mountRw`: the host owns the URL. Defaults to false. */
    embedded?: boolean;
    /** Embedded only: where the host shows the section root. */
    basePath?: string;
    /** Embedded only: the host's location to open. */
    location?: string;
    /** Section ref for this entity (e.g. "system:default/payment-gateway"). */
    sectionRef?: string;
    /** Called when the user navigates (embedded mode only). Receives the full href. */
    onNavigate?: (href: string) => void;
    /** Custom fetch function (e.g. Backstage authenticated fetch). */
    fetchFn?: typeof fetch;
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
    basePath,
    location,
    sectionRef,
    onNavigate,
    fetchFn,
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
    basePath: untrack(() => basePath),
    apiBaseUrl: untrack(() => apiBaseUrl),
    location: untrack(() => location),
    onNavigate: untrack(() => onNavigate),
  });

  export function goto(to: string): void {
    router.gotoHostLocation(to);
  }

  const sectionRefResolver = untrack(() => resolveSectionRefs);
  const page = new PageState(apiClient, { sectionRefResolver });
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

  // An embedding host owns its tab title.
  $effect(() => {
    const title = page.data?.meta.title;
    if (title !== undefined && !router.embedded) document.title = `${title} - RW`;
  });

  const currentSectionRef = untrack(() => sectionRef);

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
    // `navigation.load` reports its own failures.
    const unsubStructureReload = liveReload.on("structure", () => {
      void navigation.load(navigation.currentSectionRef ?? currentSectionRef, {
        bypassCache: true,
      });
    });
    // Cleanup can run while init is suspended at an await; whatever init does
    // afterwards would outlive it, so it checks `signal` after each one.
    const controller = new AbortController();
    const { signal } = controller;

    void (async () => {
      let config = defaultConfig;
      try {
        config = await apiClient.fetchConfig();
      } catch (e) {
        if (import.meta.env.DEV) {
          console.warn("[App] Failed to fetch config, using defaults:", e);
        }
      }
      if (signal.aborted) return;

      if (config.liveReloadEnabled && !router.embedded) {
        liveReload.connect();
      }

      if (config.commentsEnabled) {
        comments.enabled = true;
      }
    })();

    void (async () => {
      await navigation.load(currentSectionRef);
      if (signal.aborted) return;

      router.setScopePath(navigation.tree?.scope.path ?? "");
    })();

    return () => {
      controller.abort();
      cleanupRouter?.();
      liveReload.disconnect();
      unsubStructureReload();
    };
  });

  let showKit = $derived(import.meta.env.DEV && router.path === "/_kit");
</script>

<div bind:this={rootElement} class="h-full">
  <Layout>
    {#if showKit}
      {#await import("./pages/Kit.svelte") then { default: Kit }}
        <Kit />
      {/await}
    {:else}
      <Page />
    {/if}
  </Layout>
</div>
