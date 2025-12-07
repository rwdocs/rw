<script lang="ts">
  import { onMount } from "svelte";
  import { path, initRouter } from "./stores/router";
  import Layout from "./components/Layout.svelte";
  import Home from "./pages/Home.svelte";
  import Page from "./pages/Page.svelte";
  import NotFound from "./pages/NotFound.svelte";

  onMount(() => {
    initRouter();
  });

  // Determine which page to render based on path
  const getRoute = (currentPath: string) => {
    if (currentPath === "/") return "home";
    if (currentPath.startsWith("/docs")) return "page";
    return "notfound";
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
